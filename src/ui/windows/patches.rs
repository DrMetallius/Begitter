use std::collections::HashMap;
use std::ptr::{null, null_mut};
use std::mem;
use std::cmp::max;

use failure::{self, Backtrace};
use uuid::Uuid;
use winapi::shared::basetsd::INT_PTR;
use winapi::shared::windef::{HBRUSH, HMENU, HWND, HICON, RECT, LPRECT, LPSIZE, SIZE};
use winapi::shared::minwindef::{LPARAM, UINT, WPARAM, TRUE, FALSE, HIWORD, DWORD, HINSTANCE, LRESULT, LOWORD, BOOL};
use winapi::um::winuser::{self, WM_APP, PostMessageW, DialogBoxParamW, CB_RESETCONTENT, GetDlgItem, CB_ADDSTRING, CB_SETCURSEL, CB_GETCURSEL, CB_ERR,
	WNDCLASSW, DefWindowProcW, RegisterClassW, CreateWindowExW, WS_VISIBLE, WS_CLIPCHILDREN, WS_CHILD, WS_HSCROLL, WS_VSCROLL, LB_RESETCONTENT,
	LB_ADDSTRING, SCROLLINFO, WS_BORDER, MapDialogRect, LB_SETCURSEL, EnumChildWindows, DestroyWindow, BS_AUTOCHECKBOX, WM_SETTEXT, SS_LEFTNOWORDWRAP,
	GetDC, SIF_ALL, SetScrollInfo, SB_VERT, LPSCROLLINFO, RedrawWindow, RDW_INVALIDATE, RDW_ERASE, UpdateWindow, BeginPaint, FillRect, COLOR_WINDOW,
	EndPaint, PAINTSTRUCT, LPPAINTSTRUCT, SIF_POS, GetScrollInfo, ScrollWindowEx, SW_SCROLLCHILDREN, SIF_RANGE, SIF_PAGE, SB_HORZ, LB_ERR, LB_GETCURSEL};
use winapi::ctypes::c_int;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::commctrl::WC_STATIC;
use winapi::um::wingdi::GetTextExtentPoint32W;

use ui::windows::utils::{close_dialog, get_window_client_area, set_fonts};
use ui::windows::helpers::{to_wstring, WinApiError, MessageData};
use ui::windows::text::binary_to_text;
use begitter::model::patches::{PatchesModel, PatchesViewReceiver, TargetSide};
use begitter::model::View;
use begitter::change_set::CombinedPatch;
use begitter::patch_editor::patch::Change;

const HUNKS_CLASS: &str = "hunks";
static mut CLASS_REGISTERED: bool = false;

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
	left_hunks_window: HWND,
	right_hunks_window: HWND,
}

impl PatchesView {
	pub fn show(parent: HWND, patches: Vec<CombinedPatch>) -> INT_PTR {
		unsafe {
			DialogBoxParamW(null_mut(), to_wstring("patches_dialog").as_ptr(), parent, Some(patches_dialog_proc), Box::into_raw(Box::new(patches)) as LPARAM)
		}
	}

	fn initialize(patches_model: PatchesModel<PatchesViewRelay>, patches_window: HWND) -> Result<PatchesView, WinApiError> {
		let class_name = to_wstring(HUNKS_CLASS);

		if !unsafe { CLASS_REGISTERED } {
			let wnd = WNDCLASSW {
				style: 0,
				lpfnWndProc: Some(hunks_window_proc),
				cbClsExtra: 0,
				cbWndExtra: 0,
				hInstance: 0 as HINSTANCE,
				hIcon: 0 as HICON,
				hCursor: 0 as HICON,
				hbrBackground: 0 as HBRUSH,
				lpszMenuName: null(),
				lpszClassName: class_name.as_ptr(),
			};

			try_call!(RegisterClassW(&wnd), 0);
			unsafe {
				CLASS_REGISTERED = true;
			}
		}

		let mut rect = RECT {
			left: 4,
			top: 266,
			right: 396,
			bottom: 596,
		};
		try_call!(MapDialogRect(patches_window, &mut rect as LPRECT), FALSE);

		let left_hunks_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), null(), WS_VISIBLE | WS_CHILD | WS_HSCROLL | WS_VSCROLL | WS_BORDER,
				rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, patches_window, 0 as HMENU,
				0 as HINSTANCE, null_mut()));

		let mut rect = RECT {
			left: 404,
			top: 266,
			right: 796,
			bottom: 596,
		};
		try_call!(MapDialogRect(patches_window, &mut rect as LPRECT), FALSE);

		let right_hunks_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), null(), WS_VISIBLE | WS_CHILD | WS_HSCROLL | WS_VSCROLL | WS_BORDER,
				rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, patches_window, 0 as HMENU,
				0 as HINSTANCE, null_mut()));

		set_fonts(patches_window)?;

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
			left_hunks_window,
			right_hunks_window,
		})
	}

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, failure::Error> {
		let handled = match message_data.message {
			MESSAGE_MODEL_TO_PATCHES_VIEW => {
				let message = *unsafe { Box::from_raw(message_data.l_param as *mut PatchesViewMessage) };
				match message {
					PatchesViewMessage::ViewCombinedPatches(patches, left_patch, right_patch) => self.view_combined_patches(patches, &left_patch, &right_patch)?,
					PatchesViewMessage::ViewPatches(patch, target_side) => self.view_patches(patch, target_side)?,
					PatchesViewMessage::ViewHunks(combined_patch_id_and_patch_pos, target_side) => self.view_hunks(combined_patch_id_and_patch_pos, target_side)?
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

							self.patches_model.update_combined_patch_selection(target_side, patches_list[selection as usize])?;
							true
						}
						_ => false
					}
				} else if control_handle == self.left_patches_list_box || control_handle == self.right_patches_list_box {
					match HIWORD(message_data.w_param as DWORD) {
						winuser::LBN_SELCHANGE => {
							let selection = try_send_message!(control_handle, LB_GETCURSEL, 0, 0);
							if selection == LB_ERR {
								panic!("No item is selected, yet the selection change message arrived");
							}

							let target_side = if control_handle == self.left_patches_list_box { TargetSide::Left } else { TargetSide::Right };
							self.patches_model.update_patch_selection(target_side, selection as usize)?;
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

	fn receive_hunks_window_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = match message_data.message {
			winuser::WM_ERASEBKGND => {
				let mut paint: PAINTSTRUCT = unsafe { mem::zeroed() };
				let context = try_get!(BeginPaint(message_data.h_wnd, &mut paint as LPPAINTSTRUCT));

				try_call!(FillRect(context, &paint.rcPaint, COLOR_WINDOW as HBRUSH), 0);

				unsafe {
					EndPaint(message_data.h_wnd, &paint);
				}
				true
			}
			winuser::WM_VSCROLL => {
				PatchesView::process_scrolling(message_data, ScrollDirection::Vertical)?;
				true
			}
			winuser::WM_HSCROLL => {
				PatchesView::process_scrolling(message_data, ScrollDirection::Horizontal)?;
				true
			}
			_ => false
		};

		Ok(handled)
	}

	fn process_scrolling(message_data: &MessageData, scroll_direction: ScrollDirection) -> Result<(), WinApiError> {
		let direction = match scroll_direction {
			ScrollDirection::Horizontal => SB_HORZ as c_int,
			ScrollDirection::Vertical => SB_VERT as c_int
		};

		let mut scroll_info = SCROLLINFO {
			cbSize: mem::size_of::<SCROLLINFO>() as UINT,
			fMask: SIF_ALL,
			nMin: 0,
			nMax: 0,
			nPage: 0,
			nPos: 0,
			nTrackPos: 0,
		};
		try_call!(GetScrollInfo(message_data.h_wnd, direction, &mut scroll_info as LPSCROLLINFO), FALSE);

		// Save the position for comparison later on.
		let prev_pos = scroll_info.nPos;
		match LOWORD(message_data.w_param as DWORD) as LPARAM {
			winuser::SB_TOP => scroll_info.nPos = scroll_info.nMin,
			winuser::SB_BOTTOM => scroll_info.nPos = scroll_info.nMax,
			winuser::SB_LINEUP => scroll_info.nPos -= 10,
			winuser::SB_LINEDOWN => scroll_info.nPos += 10,
			winuser::SB_PAGEUP => scroll_info.nPos -= (scroll_info.nPage / 2) as c_int,
			winuser::SB_PAGEDOWN => scroll_info.nPos += (scroll_info.nPage / 2) as c_int,
			winuser::SB_THUMBTRACK => scroll_info.nPos = scroll_info.nTrackPos,
			_ => ()
		}

		scroll_info.fMask = SIF_POS;
		unsafe {
			SetScrollInfo(message_data.h_wnd, direction, &mut scroll_info as LPSCROLLINFO, TRUE);
		}
		try_call!(GetScrollInfo(message_data.h_wnd, direction, &mut scroll_info as LPSCROLLINFO), FALSE);

		// If the position has changed, scroll window and update it.
		if scroll_info.nPos != prev_pos {
			let (hor_distance, ver_distance) = match scroll_direction {
				ScrollDirection::Horizontal => (prev_pos - scroll_info.nPos, 0),
				ScrollDirection::Vertical => (0, prev_pos - scroll_info.nPos),
			};

			try_call!(ScrollWindowEx(message_data.h_wnd, hor_distance, ver_distance, null(), null(), null_mut(), null_mut(), SW_SCROLLCHILDREN), FALSE);
			try_call!(RedrawWindow(message_data.h_wnd, null(), null_mut(), RDW_ERASE | RDW_INVALIDATE), FALSE);

			extern "system" fn enum_child_windows_callback(hwnd: HWND, _: LPARAM) -> BOOL {
				unsafe {
					EnumChildWindows(hwnd, Some(enum_child_windows_callback), 0);
				}

				let result = unsafe { UpdateWindow(hwnd) };
				if result != TRUE {
					panic!("Unable to update the child window: {:?}", WinApiError(unsafe { GetLastError() } as u64, Backtrace::new()));
				}
				TRUE
			}

			unsafe {
				EnumChildWindows(message_data.h_wnd, Some(enum_child_windows_callback), 0);
			}
		}

		Ok(())
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

	fn view_hunks(&self, combined_patch_id_and_patch_pos: Option<(Uuid, usize)>, target_side: TargetSide) -> Result<(), WinApiError> {
		let (list_box, hunks_window) = match target_side {
			TargetSide::Left => (self.left_patches_list_box, self.left_hunks_window),
			TargetSide::Right => (self.right_patches_list_box, self.right_hunks_window)
		};

		let pos = combined_patch_id_and_patch_pos.map_or(-1isize as WPARAM, |value| value.1);
		try_send_message!(list_box, LB_SETCURSEL, pos, 0);

		extern "system" fn enum_child_windows_callback(hwnd: HWND, _: LPARAM) -> BOOL {
			unsafe {
				EnumChildWindows(hwnd, Some(enum_child_windows_callback), 0);
			}

			let result = unsafe { DestroyWindow(hwnd) };
			if result != TRUE {
				panic!("Unable to destroy the child window: {:?}", WinApiError(unsafe { GetLastError() } as u64, Backtrace::new()));
			}
			TRUE
		}

		unsafe {
			EnumChildWindows(hunks_window, Some(enum_child_windows_callback), 0);
		}

		if let Some((combined_patch_id, pos)) = combined_patch_id_and_patch_pos {
			let patch = &self.patches[&combined_patch_id].patches[pos];
			match patch.change {
				Change::Addition { .. } | Change::Removal { .. } => return Ok(()),
				_ => ()
			}

			let window_area = get_window_client_area(hunks_window)?;

			let static_class = to_wstring(WC_STATIC);
			let button_class = to_wstring("Button");
			let mut top = 0;
			let mut right = 0;
			for hunk in &patch.hunks {
				try_get!(CreateWindowExW(0, button_class.as_ptr(), null(), BS_AUTOCHECKBOX | WS_VISIBLE | WS_CLIPCHILDREN | WS_CHILD,
					4, top, 16, 16, hunks_window, 0 as HMENU, 0 as HINSTANCE, null_mut()));

				let hunk_text = match binary_to_text(&hunk.data) {
					Ok(text) => text,
					Err(err) => {
						println!("Couldn't read the file text: {:?}", err); // TODO: this isn't proper handling
						return Ok(());
					}
				};
				let text = format!("{}\r\n{}", hunk.header(), hunk_text);

				let context = try_call!(GetDC(hunks_window), null_mut());
				let line_sizes = text.split("\r\n").map(|line| {
					let mut size = SIZE {
						cx: 0,
						cy: 0,
					};
					let line = to_wstring(&line);
					try_call!(GetTextExtentPoint32W(context, line.as_ptr(), line.len() as c_int, &mut size as LPSIZE), FALSE);

					Ok(size)
				}).collect::<Result<Vec<_>, WinApiError>>()?;

				let longest_line_size = line_sizes.iter().fold(&SIZE {
					cx: 0,
					cy: 0,
				}, |longest_size, size| if longest_size.cx < size.cx { size } else { longest_size });

				let text = to_wstring(&text);
				let text_height = longest_line_size.cy * line_sizes.len() as c_int;
				let hunk_window = try_get!(CreateWindowExW(0, static_class.as_ptr(), null(), SS_LEFTNOWORDWRAP | WS_VISIBLE | WS_CLIPCHILDREN | WS_CHILD,
					24, top, longest_line_size.cx, text_height, hunks_window, 0 as HMENU, 0 as HINSTANCE, null_mut()));
				try_send_message!(hunk_window, WM_SETTEXT, 0, text.as_ptr() as LPARAM);

				top += text_height;
				right = max(right, longest_line_size.cx);
			}

			set_fonts(self.patches_window)?;

			let vert_scroll_info = SCROLLINFO {
				cbSize: mem::size_of::<SCROLLINFO>() as UINT,
				fMask: SIF_PAGE | SIF_POS | SIF_RANGE,
				nMin: 0,
				nMax: top - 1,
				nPage: (window_area.bottom - window_area.top) as UINT,
				nPos: 0,
				nTrackPos: 0,
			};

			let hor_scroll_info = SCROLLINFO {
				cbSize: mem::size_of::<SCROLLINFO>() as UINT,
				fMask: SIF_PAGE | SIF_POS | SIF_RANGE,
				nMin: 0,
				nMax: right - 1,
				nPage: (window_area.right - window_area.left) as UINT,
				nPos: 0,
				nTrackPos: 0,
			};

			unsafe {
				SetScrollInfo(hunks_window, SB_VERT as c_int, &vert_scroll_info as *const _ as LPSCROLLINFO, TRUE);
				SetScrollInfo(hunks_window, SB_HORZ as c_int, &hor_scroll_info as *const _ as LPSCROLLINFO, TRUE);
			}
		}

		Ok(())
	}
}

enum ScrollDirection {
	Horizontal,
	Vertical,
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

pub extern "system" fn hunks_window_proc(h_wnd: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
	let message_data = &MessageData {
		h_wnd,
		message,
		w_param,
		l_param,
	};

	let handled = match unsafe { PATCHES_VIEW.as_mut() } {
		Some(ref mut view) => view.receive_hunks_window_message(message_data).unwrap(),
		None => false
	};

	if handled {
		return 0;
	}

	if message == winuser::WM_VSCROLL {
		println!("Vertical scroll");
	}

	unsafe {
		DefWindowProcW(h_wnd, message, w_param, l_param)
	}
}

enum PatchesViewMessage {
	ViewCombinedPatches(Vec<(Uuid, CombinedPatch)>, Option<Uuid>, Option<Uuid>),
	ViewPatches(Option<Uuid>, TargetSide),
	ViewHunks(Option<(Uuid, usize)>, TargetSide),
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

	fn view_hunks(&self, combined_patch_id_and_patch_pos: Option<(Uuid, usize)>, target_side: TargetSide) -> Result<(), failure::Error> {
		self.post_on_main_thread(PatchesViewMessage::ViewHunks(combined_patch_id_and_patch_pos, target_side)).map_err(|err| err.into())
	}
}