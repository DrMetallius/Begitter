use std::error::Error;
use std::ptr::null_mut;
use std::sync::Arc;
use failure;
use super::helpers::*;
use begitter::model::main::{MainModel, MainView};
use begitter::change_set::Commit;
use ui::text::WINDOW_NAME;
use winapi::Interface;
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{DWORD, HINSTANCE, LOWORD, LPARAM, LRESULT, UINT, WORD, WPARAM};
use winapi::shared::ntdef::HRESULT;
use winapi::shared::windef::{HBRUSH, HMENU, HWND, POINT};
use winapi::shared::winerror::S_OK;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::CoCreateInstance;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::processthreadsapi::GetCurrentThreadId;
use winapi::um::shobjidl::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, IFileDialog};
use winapi::um::shobjidl_core::{IShellItem, SIGDN_FILESYSPATH};
use winapi::um::winnt::WCHAR;
use winapi::um::winuser::{self, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW, LB_RESETCONTENT, IDI_APPLICATION, LB_ADDSTRING, LBS_NOTIFY, LB_GETCOUNT, LB_DELETESTRING, LB_ERR, LB_ERRSPACE, LoadAcceleratorsW, LoadCursorW, LoadIconW, MSG, PostQuitMessage, SendMessageW, PostThreadMessageW, PostMessageW, RegisterClassW, ShowWindow, SW_SHOWDEFAULT, TranslateAcceleratorW, TranslateMessage, WM_APP, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_CHILD, WS_BORDER, WS_TABSTOP, WS_VSCROLL};

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;
const ID_CHILD_BRANCHES_LIST: HMENU = 200 as HMENU;

const MESSAGE_OPEN_FOLDER: UINT = WM_APP;
const MESSAGE_MAIN_VIEW: UINT = WM_APP + 1;

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

	let h_wnd_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), to_wstring(WINDOW_NAME).as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
		0, 0, 500, 500, 0 as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

	let branches_list_box = try_get!(CreateWindowExW(0, to_wstring("LISTBOX").as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
		0, 0, 200, 200, h_wnd_window as HWND, ID_CHILD_BRANCHES_LIST as HMENU, 0 as HINSTANCE, null_mut()));

	let commits_list_box = try_get!(CreateWindowExW(0, to_wstring("LISTBOX").as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
		210, 0, 200, 200, h_wnd_window as HWND, ID_CHILD_BRANCHES_LIST as HMENU, 0 as HINSTANCE, null_mut()));

	unsafe {
		ShowWindow(h_wnd_window, SW_SHOWDEFAULT);
	}

	let main_view_relay = Arc::new(MainViewRelay {
		main_thread_id: unsafe { GetCurrentThreadId() }
	});

	let mut main_view = MainViewImpl::new(branches_list_box, commits_list_box);

	let mut main_model: Option<MainModel> = None;

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

				main_model = Some(MainModel::new(main_view_relay.clone(), dir))
			}
			MESSAGE_MAIN_VIEW => main_view.receive_on_main_thread(&msg)?,
			_ => ()
		}

		unsafe {
			if TranslateAcceleratorW(h_wnd_window, accelerators, &mut msg) == 0 {
				TranslateMessage(&mut msg);
				DispatchMessageW(&mut msg);
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
		_ => None
	};

	if let Some(result_code) = result {
		return result_code;
	}

	unsafe {
		DefWindowProcW(h_wnd, msg, w_param, l_param)
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
		try_call!(PostThreadMessageW(self.main_thread_id, MESSAGE_MAIN_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
		Ok(())
	}
}

enum MainViewMessage {
	Branches(Vec<String>, String),
	Commits(Vec<Commit>),
}

impl MainView for MainViewRelay {
	fn error(&self, error: failure::Error) {}

	fn show_branches(&self, branches: Vec<String>, active_branch: String) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Branches(branches, active_branch)).map_err(|err| err.into())
	}

	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Commits(commits)).map_err(|err| err.into())
	}

	fn show_edited_commits(&self, commits: &[String]) -> Result<(), failure::Error> {
		Ok(())
	}
}

struct MainViewImpl {
	branches_list_box: HWND,
	commits_list_box: HWND,

	branches: Vec<String>,
	active_branch: Option<String>,
	commits: Vec<Commit>,
}

impl MainViewImpl {
	fn new(branches_list_box: HWND, commits_list_box: HWND) -> MainViewImpl {
		MainViewImpl {
			branches_list_box,
			commits_list_box,
			branches: Vec::new(),
			active_branch: None,
			commits: Vec::new(),
		}
	}

	fn receive_on_main_thread(&mut self, message: &MSG) -> Result<(), WinApiError> {
		debug_assert_eq!(message.message, MESSAGE_MAIN_VIEW);
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
		}

		Ok(())
	}
}
