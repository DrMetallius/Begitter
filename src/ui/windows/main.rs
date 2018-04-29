use std::ptr::null_mut;
use std::sync::Arc;

use failure;
use winapi::Interface;
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{DWORD, HINSTANCE, HIWORD, LOWORD, MAKELONG, LPARAM, LRESULT, UINT, WORD, WPARAM};
use winapi::shared::windef::{HBRUSH, HMENU, HWND, POINT};
use winapi::shared::winerror::S_OK;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::CoCreateInstance;
use winapi::um::processthreadsapi::GetCurrentThreadId;
use winapi::um::shobjidl::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, IFileDialog};
use winapi::um::shobjidl_core::{IShellItem, SIGDN_FILESYSPATH};
use winapi::um::winnt::WCHAR;
use winapi::um::winuser::{self, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW, LB_RESETCONTENT, IDI_APPLICATION,
	LB_ADDSTRING, LBS_NOTIFY, LB_ERR, LB_ERRSPACE, LoadAcceleratorsW, LoadCursorW, LoadIconW, MSG, PostQuitMessage,
	PostThreadMessageW, PostMessageW, RegisterClassW, ShowWindow, SW_SHOWDEFAULT, TranslateAcceleratorW, TranslateMessage, WM_APP,
	WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_CHILD, WS_BORDER, WS_TABSTOP, WS_VSCROLL, TPM_TOPALIGN, TPM_LEFTALIGN,
	TrackPopupMenuEx, GetSubMenu, LB_SETCURSEL, TPM_RETURNCMD, LB_ITEMFROMPOINT, MapWindowPoints, SetWindowTextW};
use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};

use super::helpers::*;
use begitter::model::main::{MainModel, MainView};
use begitter::change_set::{Commit, ChangeSetInfo};
use ui::windows::text::{load_string, STRING_MAIN_WINDOW_NAME, STRING_MAIN_BRANCHES, STRING_MAIN_PATCHES, STRING_MAIN_COMMITS};
use ui::windows::utils::set_fonts;

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MANI_MENU_COMMIT: &str = "main_commit_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;
const ID_MENU_IMPORT: WORD = 200;

const MESSAGE_OPEN_FOLDER: UINT = WM_APP;
const MESSAGE_MODEL_TO_MAIN_VIEW: UINT = WM_APP + 1;
const MESSAGE_RELAYED_MESSAGE: UINT = WM_APP + 2;

const GUID_FILE_DIALOG: GUID = GUID {
	Data1: 0xdc1c5a9c,
	Data2: 0xe88a,
	Data3: 0x4dde,
	Data4: [0xa5, 0xa1, 0x60, 0xf8, 0x2a, 0x20, 0xae, 0xf7],
};

pub fn run() -> Result<(), WinApiError> {
	let main_menu = to_wstring(MAIN_MENU);
	let class_name = to_wstring(MAIN_CLASS);
	let wnd = WNDCLASSW {
		style: 0,
		lpfnWndProc: Some(window_proc),
		cbClsExtra: 0,
		cbWndExtra: 0,
		hInstance: 0 as HINSTANCE,
		hIcon: try_get!(LoadIconW(0 as HINSTANCE, IDI_APPLICATION)),
		hCursor: try_get!(LoadCursorW(0 as HINSTANCE, IDC_ARROW)),
		hbrBackground: 16 as HBRUSH,
		lpszMenuName: main_menu.as_ptr(),
		lpszClassName: class_name.as_ptr(),
	};

	try_call!(RegisterClassW(&wnd), 0);

	let main_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), load_string(STRING_MAIN_WINDOW_NAME)?.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
		0, 0, 500, 500, 0 as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

	unsafe {
		ShowWindow(main_window, SW_SHOWDEFAULT);
	}

	let main_view_relay = Arc::new(MainViewRelay {
		main_thread_id: unsafe { GetCurrentThreadId() }
	});

	let mut main_view = MainViewImpl::initialize(main_window)?;

	let accelerators = try_get!(LoadAcceleratorsW(null_mut(), to_wstring(MAIN_ACCELERATORS).as_ptr()));

	let mut msg = MSG {
		hwnd: 0 as HWND,
		message: 0 as UINT,
		wParam: 0 as WPARAM,
		lParam: 0 as LPARAM,
		time: 0 as DWORD,
		pt: POINT { x: 0, y: 0 },
	};

	loop {
		let result = try_call!(GetMessageW(&mut msg, 0 as HWND, 0, 0), -1);
		if result == 0 {
			break;
		}

		match msg.message {
			MESSAGE_OPEN_FOLDER => {
				let dir = unsafe {
					*Box::from_raw(msg.lParam as *mut String)
				};
				main_view.set_model(MainModel::new(main_view_relay.clone(), dir));
			}
			MESSAGE_MODEL_TO_MAIN_VIEW => main_view.receive_model_message_on_main_thread(&msg)?,
			MESSAGE_RELAYED_MESSAGE => {
				let message_data = MessageData::unpack(msg.lParam);
				let handled = main_view.receive_system_message(&message_data)?;
				if !handled {
					msg.lParam = Box::into_raw(Box::new(message_data)) as LPARAM;
					unsafe {
						DispatchMessageW(&mut msg);
					}
				}
			}
			_ => {
				unsafe {
					if TranslateAcceleratorW(main_window, accelerators, &mut msg) == 0 {
						TranslateMessage(&mut msg);
						DispatchMessageW(&mut msg);
					}
				}
			}
		}
	}
	Ok(())
}

pub extern "system" fn window_proc(h_wnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
	let result = match msg {
		winuser::WM_DESTROY => {
			unsafe {
				PostQuitMessage(0);
			}
			Some(0)
		}
		winuser::WM_COMMAND => {
			match LOWORD(w_param as u32) {
				ID_MENU_OPEN => {
					if let Ok(dir) = show_open_file_dialog(h_wnd) {
						unsafe {
							PostMessageW(h_wnd, MESSAGE_OPEN_FOLDER, 0, Box::into_raw(Box::new(dir)) as LPARAM);
						}
					}
					Some(0)
				}
				_ => None
			}
		}
		winuser::WM_CONTEXTMENU => {
			unsafe {
				PostMessageW(h_wnd, MESSAGE_RELAYED_MESSAGE, 0, MessageData::pack(h_wnd, msg, w_param, l_param));
			}
			Some(0)
		}
		MESSAGE_RELAYED_MESSAGE => {
			let message_data = MessageData::unpack(l_param);
			unsafe {
				DefWindowProcW(message_data.h_wnd, message_data.message, message_data.w_param, message_data.l_param);
			}
			Some(0)
		}
		_ => None
	};

	if let Some(result_code) = result {
		return result_code;
	}

	unsafe {
		DefWindowProcW(h_wnd, msg, w_param, l_param)
	}
}

struct MessageData {
	h_wnd: HWND,
	message: UINT,
	w_param: WPARAM,
	l_param: LPARAM,
}

impl MessageData {
	fn pack(h_wnd: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> LPARAM {
		let data = MessageData {
			h_wnd,
			message,
			w_param,
			l_param,
		};
		Box::into_raw(Box::new(data)) as LPARAM
	}

	fn unpack(l_param: LPARAM) -> MessageData {
		let data_box = unsafe {
			Box::from_raw(l_param as *mut _)
		};
		*data_box
	}

	fn to_message(&self, msg: &mut MSG) {
		msg.hwnd = self.h_wnd;
		msg.message = self.message;
		msg.wParam = self.w_param;
		msg.lParam = self.l_param;
	}
}

fn show_open_file_dialog(owner: HWND) -> Result<String, WinApiError> {
	try_com!(CoCreateInstance(&GUID_FILE_DIALOG,
		null_mut(),
		CLSCTX_INPROC_SERVER,
		&IFileDialog::uuidof() as *const _,
		com_out file_dialog: IFileDialog));

	try_com!(file_dialog.GetOptions(out options));
	try_com!(file_dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM));
	try_com!(file_dialog.Show(owner));
	try_com!(file_dialog.GetResult(com_out dialog_result: IShellItem));

	try_com!(dialog_result.GetDisplayName(SIGDN_FILESYSPATH, com_mem_out display_name: WCHAR));
	Ok(from_wstring(&mut *display_name as *mut _))
}

struct MainViewRelay {
	main_thread_id: DWORD
}

impl MainViewRelay {
	fn post_on_main_thread(&self, message: MainViewMessage) -> Result<(), WinApiError> {
		let message = Box::new(message);
		try_call!(PostThreadMessageW(self.main_thread_id, MESSAGE_MODEL_TO_MAIN_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
		Ok(())
	}
}

enum MainViewMessage {
	Branches(Vec<String>, String),
	Commits(Vec<Commit>),
	CombinedPatches(Vec<ChangeSetInfo>),
}

impl MainView for MainViewRelay {
	fn error(&self, error: failure::Error) {
		println!("We've got an error: {}\n{}", error, error.backtrace()); // TODO: this is not proper error handling
	}

	fn show_branches(&self, branches: Vec<String>, active_branch: String) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Branches(branches, active_branch)).map_err(|err| err.into())
	}

	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Commits(commits)).map_err(|err| err.into())
	}

	fn show_combined_patches(&self, combined_patches: Vec<ChangeSetInfo>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::CombinedPatches(combined_patches)).map_err(|err| err.into())
	}
}

struct MainViewImpl {
	model: Option<MainModel>,

	main_window: HWND,
	branches_list_box: HWND,
	commits_list_box: HWND,
	combined_patches_list_box: HWND,

	branches: Vec<String>,
	active_branch: Option<String>,
	commits: Vec<Commit>,
	combined_patches: Vec<ChangeSetInfo>,
}

impl MainViewImpl {
	fn initialize(main_window: HWND) -> Result<MainViewImpl, WinApiError> {
		let branches_label = try_get!(CreateWindowExW(0, to_wstring("STATIC").as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD, 7, 5, 100, 25,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(branches_label, load_string(STRING_MAIN_BRANCHES)?.as_ptr()), 0);

		let patches_label = try_get!(CreateWindowExW(0, to_wstring("STATIC").as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD, 210, 5, 100, 25,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(patches_label, load_string(STRING_MAIN_PATCHES)?.as_ptr()), 0);

		let commits_label = try_get!(CreateWindowExW(0, to_wstring("STATIC").as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD, 210, 220, 100, 25,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(commits_label, load_string(STRING_MAIN_COMMITS)?.as_ptr()), 0);

		let branches_list_box = try_get!(CreateWindowExW(0, to_wstring("LISTBOX").as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				7, 30, 200, 200, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let combined_patches_list_box = try_get!(CreateWindowExW(0, to_wstring("LISTBOX").as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				210, 30, 200, 200, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let commits_list_box = try_get!(CreateWindowExW(0, to_wstring("LISTBOX").as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				210, 240, 200, 200, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		set_fonts(main_window)?;

		Ok(MainViewImpl {
			model: None,
			main_window,
			branches_list_box,
			combined_patches_list_box,
			commits_list_box,
			branches: Vec::new(),
			active_branch: None,
			combined_patches: Vec::new(),
			commits: Vec::new(),
		})
	}

	fn set_model(&mut self, model: MainModel) {
		self.model = Some(model)
	}

	fn receive_model_message_on_main_thread(&mut self, message: &MSG) -> Result<(), WinApiError> {
		debug_assert_eq!(message.message, MESSAGE_MODEL_TO_MAIN_VIEW);
		let arguments = unsafe {
			*Box::from_raw(message.lParam as *mut _)
		};

		match arguments {
			MainViewMessage::Branches(branches, active_branch) => {
				self.branches = branches;

				try_send_message!(self.branches_list_box, LB_RESETCONTENT, 0, 0);
				for branch_name in &self.branches {
					try_send_message!(self.branches_list_box, LB_ADDSTRING, 0, to_wstring(&branch_name).as_ptr() as LPARAM; LB_ERR, LB_ERRSPACE);
				}
			}
			MainViewMessage::Commits(commits) => {
				self.commits = commits;

				try_send_message!(self.commits_list_box, LB_RESETCONTENT, 0, 0);
				for commit in &self.commits {
					try_send_message!(self.commits_list_box, LB_ADDSTRING, 0, to_wstring(&commit.info.message).as_ptr() as LPARAM; LB_ERR, LB_ERRSPACE);
				}
			}
			MainViewMessage::CombinedPatches(combined_patches) => {
				self.combined_patches = combined_patches;

				try_send_message!(self.combined_patches_list_box, LB_RESETCONTENT, 0, 0);
				for info in &self.combined_patches {
					try_send_message!(self.combined_patches_list_box, LB_ADDSTRING, 0, to_wstring(&info.message).as_ptr() as LPARAM; LB_ERR, LB_ERRSPACE);
				}
			}
		}

		Ok(())
	}

	fn receive_system_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = match message_data.message {
			winuser::WM_CONTEXTMENU => {
				if message_data.w_param as HWND == self.commits_list_box {
					self.on_commit_right_click(message_data)
				} else {
					false
				}
			}
			_ => false
		};
		Ok(handled)
	}

	fn on_commit_right_click(&self, message_data: &MessageData) -> bool {
		let x = GET_X_LPARAM(message_data.l_param);
		let y = GET_Y_LPARAM(message_data.l_param);
		let POINT { x: translated_x, y: translated_y } = {
			let mut point = POINT { x, y };
			unsafe {
				MapWindowPoints(null_mut(), self.commits_list_box, &mut point as *mut _, 1);
			}
			point
		};

		let result = try_send_message!(self.commits_list_box, LB_ITEMFROMPOINT, 0, MAKELONG(translated_x as u16, translated_y as u16) as LPARAM) as u32;
		let index = LOWORD(result);
		let outside = HIWORD(result) != 0;
		if outside {
			return false;
		}

		try_send_message!(self.commits_list_box, LB_SETCURSEL, index as usize, 0) as u32;

		let context_menu = MenuHandle::load(MANI_MENU_COMMIT).unwrap();
		let result = unsafe {
			let position = 0;
			let popup = GetSubMenu(context_menu.handle(), position);
			if popup.is_null() {
				panic!("{} is an invalid menu position", position);
			}
			TrackPopupMenuEx(popup, TPM_RETURNCMD | TPM_TOPALIGN | TPM_LEFTALIGN,
				x, y, self.main_window, null_mut()) as WORD
		};

		match result {
			self::ID_MENU_IMPORT => {
				let commits: Vec<Commit> = self.commits[0..index as usize + 1]
						.iter()
						.map(|commit| commit.clone())
						.collect();
				self.model.as_ref().unwrap().import_commits(commits);
				true
			}
			_ => false
		}
	}
}
