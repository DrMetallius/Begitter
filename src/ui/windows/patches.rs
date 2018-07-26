use std::ptr::null_mut;

use failure;
use uuid::Uuid;
use winapi::shared::basetsd::INT_PTR;
use winapi::shared::windef::HWND;
use winapi::shared::minwindef::{LPARAM, UINT, WPARAM, TRUE, FALSE, HIWORD, DWORD};
use winapi::um::winuser::{self, WM_APP, PostMessageW, DialogBoxParamW, CB_RESETCONTENT, GetDlgItem, CB_ADDSTRING, CB_SETCURSEL, CB_GETCURSEL, CB_ERR};
use winapi::um::winuser::LB_RESETCONTENT;
use winapi::um::winuser::LB_ADDSTRING;
use winapi::ctypes::c_int;

use ui::windows::utils::close_dialog;
use ui::windows::helpers::{to_wstring, WinApiError, MessageData};
use begitter::model::patches::{PatchesModel, PatchesViewReceiver, TargetSide};
use begitter::model::View;
use begitter::change_set::CombinedPatch;
use begitter::patch_editor::patch::Change;
use std::iter::Map;
use std::collections::HashMap;

const ID_LEFT_PATCHES_COMBO_BOX: c_int = 10;
const ID_RIGHT_PATCHES_COMBO_BOX: c_int = 11;
const ID_LEFT_PATCHES_LIST_BOX: c_int = 12;
const ID_RIGHT_PATCHES_LIST_BOX: c_int = 13;

const MESSAGE_MODEL_TO_PATCHES_VIEW: UINT = WM_APP;

static mut PATCHES_VIEW: Option<PatchesView> = None;

pub struct PatchesView {
	patches_model: PatchesModel<PatchesViewRelay>,
	patches_window: HWND,

	patches: HashMap<Uuid, CombinedPatch>,
	patches_left: Vec<Uuid>,
	patches_right: Vec<Uuid>,

	left_patches_combo_box: HWND,
	right_patches_combo_box: HWND,
	left_patches_list_box: HWND,
	right_patches_list_box: HWND,
}

impl PatchesView {
	pub fn show(parent: HWND, patches: Vec<CombinedPatch>) -> INT_PTR {
		unsafe {
			DialogBoxParamW(null_mut(), to_wstring("patches_dialog").as_ptr(), parent, Some(patches_dialog_proc), Box::into_raw(Box::new(patches)) as LPARAM)
		}
	}

	fn initialize(patches_model: PatchesModel<PatchesViewRelay>, patches_window: HWND) -> Result<PatchesView, WinApiError> {
		Ok(PatchesView {
			patches_model,
			patches_window,
			patches: HashMap::new(),
			patches_left: Vec::new(),
			patches_right: Vec::new(),
			left_patches_combo_box: try_get!(GetDlgItem(patches_window, ID_LEFT_PATCHES_COMBO_BOX)),
			right_patches_combo_box: try_get!(GetDlgItem(patches_window, ID_RIGHT_PATCHES_COMBO_BOX)),
			left_patches_list_box: try_get!(GetDlgItem(patches_window, ID_LEFT_PATCHES_LIST_BOX)),
			right_patches_list_box: try_get!(GetDlgItem(patches_window, ID_RIGHT_PATCHES_LIST_BOX)),
		})
	}

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, failure::Error> {
		let handled = match message_data.message {
			MESSAGE_MODEL_TO_PATCHES_VIEW => {
				let message = *unsafe { Box::from_raw(message_data.l_param as *mut PatchesViewMessage) };
				match message {
					PatchesViewMessage::ViewCombinedPatches(patches, left_patch, right_patch) => self.view_combined_patches(patches, &left_patch, &right_patch)?,
					PatchesViewMessage::ViewPatches(patch, target_side) => self.view_patches(patch, target_side)?
				}
				true
			}
			winuser::WM_COMMAND => {
				let control_handle = message_data.l_param as HWND;
				if control_handle == self.left_patches_combo_box || control_handle == self.right_patches_combo_box {
					match HIWORD(message_data.w_param as DWORD) {
						winuser::CBN_SELCHANGE => {
							let selection = try_send_message!(control_handle, CB_GETCURSEL, 0, 0);
							if selection == CB_ERR {
								panic!("No item is selected, yet the selection change message arrived");
							}

							let (target_side, patches_list) = if control_handle == self.left_patches_combo_box {
								(TargetSide::Left, &self.patches_left)
							} else {
								(TargetSide::Right, &self.patches_right)
							};

							self.patches_model.update_selection(target_side, patches_list[selection as usize])?;
							true
						}
						_ => false
					}
				} else {
					false
				}
			}
			_ => false
		};
		Ok(handled)
	}

	fn view_combined_patches(&mut self, patches: Vec<(Uuid, CombinedPatch)>, left_patch: &Option<Uuid>, right_patch: &Option<Uuid>) -> Result<(), WinApiError> {
		{
			let fill_out = |combo_box: HWND, selected_id: &Option<Uuid>, skipped_id: &Option<Uuid>| -> Result<Vec<Uuid>, WinApiError> {
				try_send_message!(combo_box, CB_RESETCONTENT, 0, 0);

				let mut ids = Vec::new();
				for (uuid, patch) in &patches {
					if let Some(skipped_id) = *skipped_id {
						if skipped_id == *uuid {
							continue;
						}
					}

					let string = to_wstring(&patch.info.message);
					try_send_message!(combo_box, CB_ADDSTRING, 0, string.as_ptr() as LPARAM);

					ids.push(*uuid);
				}

				if let Some(selected_id) = selected_id {
					let position = ids.iter().position(|uuid| *uuid == *selected_id).unwrap();
					try_send_message!(combo_box, CB_SETCURSEL, position as WPARAM, 0);
				}

				return Ok(ids);
			};

			self.patches_left = fill_out(self.left_patches_combo_box, left_patch, &None)?;
			self.patches_right = fill_out(self.right_patches_combo_box, right_patch, left_patch)?;
		}

		self.patches = patches.into_iter().collect();

		Ok(())
	}

	fn view_patches(&self, combined_patch_id: Option<Uuid>, target_side: TargetSide) -> Result<(), WinApiError> {
		let list_box = match target_side {
			TargetSide::Left => self.left_patches_list_box,
			TargetSide::Right => self.right_patches_list_box,
		};

		try_send_message!(list_box, LB_RESETCONTENT, 0, 0);

		if let Some(ref combined_patch_id) = combined_patch_id {
			let combined_patch = &self.patches[combined_patch_id];
			for patch in &combined_patch.patches {
				let (change_str, properties) = match patch.change {
					Change::Addition { ref new_properties, .. } => ("+", new_properties),
					Change::Removal { ref old_properties, .. } => ("-", old_properties),
					Change::Modification { ref new_properties, .. } => ("~", new_properties)
				};
				let name = to_wstring(&format!("{} {}", change_str, &properties.name));

				try_send_message!(list_box, LB_ADDSTRING, 0, name.as_ptr() as LPARAM);
			}
		}

		Ok(())
	}
}

pub extern "system" fn patches_dialog_proc(hwnd_dlg: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> INT_PTR {
	let handled = match message {
		winuser::WM_INITDIALOG => {
			let relay = PatchesViewRelay { patches_window: hwnd_dlg };
			let patches = *unsafe { Box::from_raw(l_param as *mut Vec<CombinedPatch>) };
			let model = PatchesModel::new(relay, patches);

			let mut view = unsafe {
				PATCHES_VIEW = Some(PatchesView::initialize(model, hwnd_dlg).unwrap());
				PATCHES_VIEW.as_ref().unwrap()
			};

			view.patches_model.initialize().unwrap();
			true
		}
		winuser::WM_CLOSE => {
			let result = Box::into_raw(Box::new(None::<Vec<CombinedPatch>>));
			close_dialog(hwnd_dlg, result as INT_PTR).unwrap();
			true
		}
		_ => {
			let message_data = &MessageData {
				h_wnd: hwnd_dlg,
				message,
				w_param,
				l_param,
			};
			match unsafe { PATCHES_VIEW.as_mut() } {
				Some(ref mut view) => view.receive_message(message_data).unwrap(),
				None => false
			}
		}
	};

	(if handled { TRUE } else { FALSE }) as INT_PTR
}

enum PatchesViewMessage {
	ViewCombinedPatches(Vec<(Uuid, CombinedPatch)>, Option<Uuid>, Option<Uuid>),
	ViewPatches(Option<Uuid>, TargetSide),
}

struct PatchesViewRelay {
	patches_window: HWND
}

impl PatchesViewRelay {
	fn post_on_main_thread(&self, message: PatchesViewMessage) -> Result<(), WinApiError> {
		let message = Box::new(message);
		try_call!(PostMessageW(self.patches_window, MESSAGE_MODEL_TO_PATCHES_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
		Ok(())
	}
}

unsafe impl Send for PatchesViewRelay {}

unsafe impl Sync for PatchesViewRelay {}

impl View for PatchesViewRelay {
	fn error(&self, error: ::failure::Error) {
		println!("We've got an error: {}\n{}", error, error.backtrace()); // TODO: this is not proper error handling
	}
}

impl PatchesViewReceiver for PatchesViewRelay {
	fn view_combined_patches(&self, patches: Vec<(Uuid, CombinedPatch)>, left_side_patch: Option<Uuid>, right_side_patch: Option<Uuid>) -> Result<(), failure::Error> {
		self.post_on_main_thread(PatchesViewMessage::ViewCombinedPatches(patches, left_side_patch, right_side_patch)).map_err(|err| err.into())
	}

	fn view_patches(&self, patch: Option<Uuid>, target_side: TargetSide) -> Result<(), failure::Error> {
		self.post_on_main_thread(PatchesViewMessage::ViewPatches(patch, target_side)).map_err(|err| err.into())
	}
}