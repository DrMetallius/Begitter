use std::ptr::null_mut;

use winapi::shared::basetsd::INT_PTR;
use winapi::shared::windef::HWND;
use winapi::shared::minwindef::{LPARAM, UINT, WPARAM, TRUE, FALSE};
use winapi::um::winuser::{self, WM_APP, PostMessageW, DialogBoxParamW};

use ui::windows::utils::close_dialog;
use ui::windows::helpers::{to_wstring, WinApiError, MessageData};

use begitter::model::patches::{PatchesModel, PatchesViewReceiver};
use begitter::model::View;
use begitter::change_set::CombinedPatch;

const MESSAGE_MODEL_TO_PATCHES_VIEW: UINT = WM_APP;

static mut PATCHES_VIEW: Option<PatchesView> = None;

pub struct PatchesView {
	patches_model: PatchesModel<PatchesViewRelay>,
	patches_window: HWND,
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
		})
	}

	fn receive_message(&self, message_data: &MessageData) -> Result<bool, WinApiError> {
		Ok(false)
	}
}

pub extern "system" fn patches_dialog_proc(hwnd_dlg: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> INT_PTR {
	let handled = match message {
		winuser::WM_INITDIALOG => {
			let relay = PatchesViewRelay { rejects_window: hwnd_dlg };
			let patches = *unsafe { Box::from_raw(l_param as *mut Vec<CombinedPatch>) };
			let model = PatchesModel::new(relay, patches);

			let mut view = unsafe {
				PATCHES_VIEW = Some(PatchesView::initialize(model,hwnd_dlg).unwrap());
				PATCHES_VIEW.as_ref().unwrap()
			};
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
			match unsafe { PATCHES_VIEW.as_ref() } {
				Some(ref view) => view.receive_message(message_data).unwrap(),
				None => false
			}
		}
	};

	(if handled { TRUE } else { FALSE }) as INT_PTR
}

enum PatchesViewMessage {}

struct PatchesViewRelay {
	rejects_window: HWND
}

impl PatchesViewRelay {
	fn post_on_main_thread(&self, message: PatchesViewMessage) -> Result<(), WinApiError> {
		let message = Box::new(message);
		try_call!(PostMessageW(self.rejects_window, MESSAGE_MODEL_TO_PATCHES_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
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
}