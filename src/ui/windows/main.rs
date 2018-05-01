use std::ptr::null_mut;
use std::sync::Arc;

use failure;
use winapi::Interface;
use winapi::ctypes::c_int;
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{DWORD, HINSTANCE, HIWORD, LOWORD, MAKELONG, LPARAM, LRESULT, UINT, WORD, WPARAM, TRUE};
use winapi::shared::windef::{HBRUSH, HMENU, HWND, POINT, RECT};
use winapi::shared::winerror::S_OK;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::CoCreateInstance;
use winapi::um::shobjidl::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, IFileDialog};
use winapi::um::shobjidl_core::{IShellItem, SIGDN_FILESYSPATH};
use winapi::um::winnt::WCHAR;
use winapi::um::winuser::{self, AdjustWindowRectExForDpi, GetWindowLongW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
	IDC_ARROW, LB_RESETCONTENT, IDI_APPLICATION, GWL_STYLE, GWL_EXSTYLE, SWP_NOMOVE,
	LB_ADDSTRING, LBS_NOTIFY, LB_ERR, LB_ERRSPACE, LoadAcceleratorsW, LoadCursorW, LoadIconW, MSG, PostQuitMessage,
	PostMessageW, RegisterClassW, ShowWindow, SetWindowPos, SW_SHOWDEFAULT, TranslateAcceleratorW, TranslateMessage, WM_APP,
	WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_CHILD, WS_BORDER, WS_TABSTOP, WS_VSCROLL, TPM_TOPALIGN, TPM_LEFTALIGN, WS_CLIPCHILDREN,
	TrackPopupMenuEx, GetSubMenu, LB_SETCURSEL, TPM_RETURNCMD, LB_ITEMFROMPOINT, MapWindowPoints, SetWindowTextW};
use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};

use super::helpers::*;
use begitter::model::main::{MainModel, MainViewReceiver};
use begitter::change_set::{Commit, ChangeSetInfo};
use ui::windows::text::{load_string, STRING_MAIN_WINDOW_NAME, STRING_MAIN_BRANCHES, STRING_MAIN_PATCHES, STRING_MAIN_COMMITS};
use ui::windows::utils::{set_fonts, get_window_position};
use ui::windows::dpi::GetDpiForWindow;

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MANI_MENU_COMMIT: &str = "main_commit_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;
const ID_MENU_IMPORT: WORD = 200;

const MESSAGE_MODEL_TO_MAIN_VIEW: UINT = WM_APP;

const GUID_FILE_DIALOG: GUID = GUID {
	Data1: 0xdc1c5a9c,
	Data2: 0xe88a,
	Data3: 0x4dde,
	Data4: [0xa5, 0xa1, 0x60, 0xf8, 0x2a, 0x20, 0xae, 0xf7],
};

static mut MAIN_VIEW: Option<MainView> = None;
static mut MAIN_VIEW_RELAY: Option<Arc<MainViewRelay>> = None;

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

	let main_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), load_string(STRING_MAIN_WINDOW_NAME)?.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
		0, 0, 500, 500, 0 as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

	unsafe {
		ShowWindow(main_window, SW_SHOWDEFAULT);

		MAIN_VIEW_RELAY = Some(Arc::new(MainViewRelay {
			main_window
		}));
		MAIN_VIEW = Some(MainView::initialize(main_window)?);
	}

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

		unsafe {
			if TranslateAcceleratorW(main_window, accelerators, &mut msg) == 0 {
				TranslateMessage(&mut msg);
				DispatchMessageW(&mut msg);
			}
		}
	}
	Ok(())
}

pub extern "system" fn window_proc(h_wnd: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
	let result = match message {
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
							MAIN_VIEW.as_mut().unwrap().set_model(MainModel::new(MAIN_VIEW_RELAY.as_mut().unwrap().clone(), dir));
						}
					}
					Some(0)
				}
				_ => None
			}
		}
		_ => {
			let handled = unsafe {
				match MAIN_VIEW.as_mut() {
					Some(view) => {
						let message_data = MessageData {
							h_wnd,
							message,
							w_param,
							l_param
						};
						view.receive_message(&message_data).unwrap()
					},
					None => false
				}
			};
			if handled { Some(0) } else { None }
		}
	};

	if let Some(result_code) = result {
		return result_code;
	}

	unsafe {
		DefWindowProcW(h_wnd, message, w_param, l_param)
	}
}

struct MessageData {
	h_wnd: HWND,
	message: UINT,
	w_param: WPARAM,
	l_param: LPARAM,
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
	main_window: HWND
}

impl MainViewRelay {
	fn post_on_main_thread(&self, message: MainViewMessage) -> Result<(), WinApiError> {
		let message = Box::new(message);
		try_call!(PostMessageW(self.main_window, MESSAGE_MODEL_TO_MAIN_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
		Ok(())
	}
}

unsafe impl Send for MainViewRelay {}

unsafe impl Sync for MainViewRelay {}

impl MainViewReceiver for MainViewRelay {
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

enum MainViewMessage {
	Branches(Vec<String>, String),
	Commits(Vec<Commit>),
	CombinedPatches(Vec<ChangeSetInfo>),
}

struct MainView {
	model: Option<MainModel>,

	main_window: HWND,
	commits_label: HWND,
	branches_list_box: HWND,
	commits_list_box: HWND,
	combined_patches_list_box: HWND,

	branches: Vec<String>,
	active_branch: Option<String>,
	commits: Vec<Commit>,
	combined_patches: Vec<ChangeSetInfo>,
}

impl MainView {
	const LABEL_WIDTH: c_int = 100;
	const EDGE_MARGIN: c_int = 7;
	const BRANCHES_WIDTH: c_int = 200;
	const SEPARATOR_WIDTH: c_int = 5;
	const COMMIT_AND_PATCH_HOR_POSITION: c_int = MainView::EDGE_MARGIN + MainView::BRANCHES_WIDTH + MainView::SEPARATOR_WIDTH;
	const LABEL_HEIGHT: c_int = 25;

	fn initialize(main_window: HWND) -> Result<MainView, WinApiError> {
		let static_class = to_wstring("STATIC");
		let list_box_class = to_wstring("LISTBOX");

		let branches_label = try_get!(CreateWindowExW(0, static_class.as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD,
				MainView::EDGE_MARGIN, MainView::EDGE_MARGIN, MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(branches_label, load_string(STRING_MAIN_BRANCHES)?.as_ptr()), 0);

		let patches_label = try_get!(CreateWindowExW(0, static_class.as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD,
				MainView::COMMIT_AND_PATCH_HOR_POSITION, MainView::EDGE_MARGIN, MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(patches_label, load_string(STRING_MAIN_PATCHES)?.as_ptr()), 0);

		let commits_label = try_get!(CreateWindowExW(0, static_class.as_ptr(), null_mut(), WS_VISIBLE | WS_CHILD, 0, 0, MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(commits_label, load_string(STRING_MAIN_COMMITS)?.as_ptr()), 0);

		let branches_list_box = try_get!(CreateWindowExW(0, list_box_class.as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				MainView::EDGE_MARGIN, MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(branches_label, load_string(STRING_MAIN_BRANCHES)?.as_ptr()), 0);

		let combined_patches_list_box = try_get!(CreateWindowExW(0, list_box_class.as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				MainView::COMMIT_AND_PATCH_HOR_POSITION, MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let commits_list_box = try_get!(CreateWindowExW(0, list_box_class.as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				0, 0, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		set_fonts(main_window)?;

		let view = MainView {
			model: None,
			main_window,
			commits_label,
			branches_list_box,
			combined_patches_list_box,
			commits_list_box,
			branches: Vec::new(),
			active_branch: None,
			combined_patches: Vec::new(),
			commits: Vec::new(),
		};

		let rect = get_window_position(main_window, main_window)?;
		view.reposition_views(rect.right - rect.left, rect.bottom - rect.top)?;

		Ok(view)
	}

	fn set_model(&mut self, model: MainModel) {
		self.model = Some(model)
	}

	fn receive_model_message_on_main_thread(&mut self, message_data: &MessageData) -> Result<(), WinApiError> {
		debug_assert_eq!(message_data.message, MESSAGE_MODEL_TO_MAIN_VIEW);
		let arguments = unsafe {
			*Box::from_raw(message_data.l_param as *mut _)
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

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = match message_data.message {
			winuser::WM_CONTEXTMENU => {
				if message_data.w_param as HWND == self.commits_list_box {
					self.on_commit_right_click(message_data)
				} else {
					false
				}
			}
			winuser::WM_SIZING => {
				let rect = unsafe { *(message_data.l_param as *const RECT) };
				let width = rect.right - rect.left;
				let height = rect.bottom - rect.top;
				self.reposition_views(width, height)?;
				true
			}
			MESSAGE_MODEL_TO_MAIN_VIEW => {
				self.receive_model_message_on_main_thread(message_data)?;
				true
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

	fn reposition_views(&self, width: c_int, height: c_int) -> Result<(), WinApiError> {
		let style = try_call!(GetWindowLongW(self.main_window, GWL_STYLE), 0) as DWORD;
		let extended_style = try_call!(GetWindowLongW(self.main_window, GWL_EXSTYLE), 0) as DWORD;
		let dpi = unsafe { GetDpiForWindow(self.main_window) };

		let mut rect = RECT {
			top: 0,
			left: 0,
			right: 0,
			bottom: 0
		};
		try_call!(AdjustWindowRectExForDpi(&mut rect as *mut _ as *mut _, style, TRUE, extended_style, dpi), 0);

		let hor_diff = rect.right - rect.left;
		let client_area_width = width - hor_diff;
		let vert_diff = rect.bottom - rect.top;
		let client_area_height = height - vert_diff;

		try_call!(SetWindowPos(self.branches_list_box, null_mut(), 0, 0, MainView::BRANCHES_WIDTH, client_area_height - 2 * MainView::EDGE_MARGIN - MainView::LABEL_HEIGHT, SWP_NOMOVE), 0);

		let patches_height = (client_area_height  - 2 * (MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT)) / 2;
		let commits_and_patches_hor_pos = client_area_width - MainView::COMMIT_AND_PATCH_HOR_POSITION - MainView::EDGE_MARGIN;
		let commits_vert_pos = MainView::EDGE_MARGIN + 2 * MainView::LABEL_HEIGHT + patches_height;
		try_call!(SetWindowPos(self.combined_patches_list_box, null_mut(), 0, 0, commits_and_patches_hor_pos, patches_height, SWP_NOMOVE), 0);
		try_call!(SetWindowPos(self.commits_list_box, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos, commits_and_patches_hor_pos,
				client_area_height - commits_vert_pos - MainView::EDGE_MARGIN, 0), 0);
		try_call!(SetWindowPos(self.commits_label, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos - MainView::LABEL_HEIGHT,
				MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT, 0), 0);
		Ok(())
	}
}
