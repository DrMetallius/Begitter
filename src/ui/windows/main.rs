use std::ptr::null_mut;
use std::sync::Arc;
use std::mem;

use failure;
use time;
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
	TrackPopupMenuEx, GetSubMenu, LB_SETCURSEL, TPM_RETURNCMD, LB_ITEMFROMPOINT, MapWindowPoints, SetWindowTextW, LPNMHDR, SWP_NOREDRAW};
use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};
use winapi::um::commctrl::{self, WC_TREEVIEW, WC_LISTBOX, WC_STATIC, TVS_HASLINES, TVM_INSERTITEMW, TVINSERTSTRUCTW, TVI_SORT, TVIF_TEXT,
	TVM_DELETEITEM, TVI_ROOT, TVIF_CHILDREN, HTREEITEM, TVIF_STATE, TVIS_BOLD, TVS_HASBUTTONS, TVS_LINESATROOT, TVIS_EXPANDED, TVM_GETNEXTITEM,
	TVGN_CARET, TVIF_PARAM, TVITEMEXW, TVM_GETITEMW, TVIF_HANDLE, LVM_INSERTCOLUMNW, NMLVDISPINFOW, NMITEMACTIVATE, WC_LISTVIEW, LVM_DELETEALLITEMS,
	LVCF_TEXT, LVCF_SUBITEM, LVCOLUMNW, LVCF_WIDTH, LVIF_TEXT, LPSTR_TEXTCALLBACKW, LVM_INSERTITEMW, LVITEMW, LVN_GETDISPINFOW, LVS_REPORT, LVS_LIST,
	LVIF_COLUMNS, LVCF_FMT, LVCFMT_LEFT, LVIF_STATE};

use super::helpers::*;
use begitter::model::main::{BranchItem, MainModel, MainViewReceiver};
use begitter::change_set::{Commit, ChangeSetInfo};
use ui::windows::text::{load_string, STRING_MAIN_WINDOW_NAME, STRING_MAIN_BRANCHES, STRING_MAIN_PATCHES, STRING_MAIN_COMMITS, STRING_MAIN_COMMITS_COLUMNS};
use ui::windows::utils::{set_fonts, get_window_position};
use ui::windows::dpi::GetDpiForWindow;

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MANI_MENU_COMMIT: &str = "main_commit_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;
const ID_MENU_IMPORT: WORD = 200;
const ID_MENU_APPLY: WORD = 201;

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
							l_param,
						};
						view.receive_message(&message_data).unwrap()
					}
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

	fn show_branches(&self, branches: Vec<BranchItem>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Branches(branches)).map_err(|err| err.into())
	}

	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Commits(commits)).map_err(|err| err.into())
	}

	fn show_combined_patches(&self, combined_patches: Vec<ChangeSetInfo>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::CombinedPatches(combined_patches)).map_err(|err| err.into())
	}
}

enum MainViewMessage {
	Branches(Vec<BranchItem>),
	Commits(Vec<Commit>),
	CombinedPatches(Vec<ChangeSetInfo>),
}

struct MainView {
	model: Option<MainModel>,

	main_window: HWND,
	commits_label: HWND,
	branches_tree_view: HWND,
	commits_list_view: HWND,
	combined_patches_list_box: HWND,

	branches: Vec<BranchItem>,
	active_branch: Option<String>,
	commits: Vec<Commit>,
	commit_strings: Vec<Vec<Vec<u16>>>,
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
		let static_class = to_wstring(WC_STATIC);
		let tree_view_class = to_wstring(WC_TREEVIEW);
		let list_box_class = to_wstring(WC_LISTBOX);
		let list_view_class = to_wstring(WC_LISTVIEW);

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

		let branches_tree_view = try_get!(CreateWindowExW(0, tree_view_class.as_ptr(), null_mut(), TVS_LINESATROOT | TVS_HASBUTTONS | TVS_HASLINES | WS_TABSTOP | WS_BORDER |
				WS_VISIBLE | WS_CHILD | WS_VSCROLL, MainView::EDGE_MARGIN, MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT, 0, 0,
				main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let combined_patches_list_box = try_get!(CreateWindowExW(0, list_box_class.as_ptr(), null_mut(), WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | LBS_NOTIFY | WS_VSCROLL,
				MainView::COMMIT_AND_PATCH_HOR_POSITION, MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let commits_list_view = try_get!(CreateWindowExW(0, list_view_class.as_ptr(), null_mut(), LVS_REPORT | WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | WS_VSCROLL,
				0, 0, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

		let mut column: LVCOLUMNW = unsafe { mem::zeroed() };
		column.mask = LVCF_TEXT | LVCF_SUBITEM | LVCF_WIDTH | LVCF_FMT;

		for (index, text_id) in STRING_MAIN_COMMITS_COLUMNS.enumerate() {

			let mut name = load_string(text_id)?;
			column.fmt = LVCFMT_LEFT;
			column.iSubItem = index as c_int;
			column.pszText = name.as_mut_ptr();
			column.cx = 200;
			try_send_message!(commits_list_view, LVM_INSERTCOLUMNW, index, &mut column as *mut _ as LPARAM; -1);
		}

		set_fonts(main_window)?;

		let view = MainView {
			model: None,
			main_window,
			commits_label,
			branches_tree_view,
			combined_patches_list_box,
			commits_list_view,
			branches: Vec::new(),
			active_branch: None,
			combined_patches: Vec::new(),
			commits: Vec::new(),
			commit_strings: Vec::new(),
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
			MainViewMessage::Branches(branches) => {
				self.branches = branches;

				try_send_message!(self.branches_tree_view, TVM_DELETEITEM, 0, TVI_ROOT as LPARAM; 0);
				self.view_branches_recursively(&self.branches, null_mut())?;
			}
			MainViewMessage::Commits(commits) => {
				self.commits = commits;
				self.commit_strings.clear();

				try_send_message!(self.commits_list_view, LVM_DELETEALLITEMS, 0, 0);

				let mut item: LVITEMW = unsafe { mem::zeroed() };
				item.mask = LVIF_TEXT | LVIF_STATE;
				item.pszText = LPSTR_TEXTCALLBACKW;
				item.iSubItem = 0;
				item.state = 0;
				item.stateMask = 0;

				for (commit_index, commit) in self.commits.iter().enumerate() {
					let commit_time = time::strftime("%Y-%m-%d %H:%M:%S", &time::at(commit.info.change_set_info.author_action.time.clone())).unwrap();
					let strings = vec![&commit.info.change_set_info.message,
						&commit.info.change_set_info.author_action.name,
						&commit_time,
						&commit.hash]
							.into_iter()
							.map(|string| to_wstring(&string))
							.collect();
					self.commit_strings.push(strings);

					item.iItem = commit_index as c_int;
					try_send_message!(self.commits_list_view, LVM_INSERTITEMW, 0, &item as *const _ as LPARAM; -1);
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

	fn view_branches_recursively(&self, branches: &Vec<BranchItem>, parent: HTREEITEM) -> Result<(), WinApiError> {
		for branch in branches {
			fn insert_item(branches_tree_view: HWND, name: &mut Vec<u16>, ref_name_ptr: *const String, has_children: bool, bold: bool, expanded: bool,
						parent: HTREEITEM) -> Result<HTREEITEM, WinApiError> {
				let mut item = TVINSERTSTRUCTW {
					hInsertAfter: TVI_SORT,
					hParent: parent,
					u: unsafe { mem::zeroed() },
				};

				{
					let item_info = unsafe { item.u.itemex_mut() };
					item_info.mask = TVIF_TEXT | TVIF_CHILDREN | TVIF_STATE | TVIF_PARAM;
					item_info.pszText = name.as_mut_ptr();
					item_info.cchTextMax = name.len() as i32;
					item_info.cChildren = if has_children { 1 } else { 0 };
					item_info.state = if bold { TVIS_BOLD } else { 0 } | if expanded { TVIS_EXPANDED } else { 0 };
					item_info.stateMask = TVIS_BOLD | TVIS_EXPANDED;
					item_info.lParam = ref_name_ptr as LPARAM;
				}

				let item_handle = try_send_message!(branches_tree_view, TVM_INSERTITEMW, 0, &item as *const _ as LPARAM; 0);
				Ok(item_handle as HTREEITEM)
			}

			match *branch {
				BranchItem::Folder { ref display_name, ref children, has_active_child } => {
					let mut branch_name_str = to_wstring(display_name.as_str());
					let handle = insert_item(self.branches_tree_view, &mut branch_name_str, null_mut(),true, false, has_active_child, parent)?;
					self.view_branches_recursively(children, handle)?;
				}
				BranchItem::Branch { ref display_name, active, ref ref_name } => {
					let mut branch_name_str = to_wstring(display_name.as_str());
					insert_item(self.branches_tree_view, &mut branch_name_str, ref_name as *const _,  false, active, false, parent)?;
				}
			};
		}

		Ok(())
	}

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = match message_data.message {
			winuser::WM_SIZING => {
				let rect = unsafe { *(message_data.l_param as *const RECT) };
				let width = rect.right - rect.left;
				let height = rect.bottom - rect.top;
				self.reposition_views(width, height)?;
				true
			}
			winuser::WM_NOTIFY => {
				let header = unsafe { *(message_data.l_param as LPNMHDR) };
				match header.code {
					commctrl::NM_DBLCLK => {
						if header.hwndFrom == self.branches_tree_view {
							self.on_branch_double_click()?
						} else {
							false
						}
					}
					commctrl::NM_RCLICK => {
						if header.hwndFrom == self.commits_list_view {
							let info = unsafe { (message_data.l_param as *const NMITEMACTIVATE).as_ref().unwrap() };
							self.on_commit_right_click(&info)
						} else {
							false
						}
					}
					commctrl::LVN_GETDISPINFOW => {
						if header.hwndFrom == self.commits_list_view {
							let mut info = unsafe { (message_data.l_param as *mut NMLVDISPINFOW).as_mut().unwrap() };
							info.item.pszText = self.commit_strings[info.item.iItem as usize][info.item.iSubItem as usize].as_mut_ptr();
							true
						} else {
							false
						}
					}
					_ => false
				}
			}
			MESSAGE_MODEL_TO_MAIN_VIEW => {
				self.receive_model_message_on_main_thread(message_data)?;
				true
			}
			_ => false
		};
		Ok(handled)
	}

	fn on_branch_double_click(&self) -> Result<bool, WinApiError> {
		let selected_item_handle= try_send_message!(self.branches_tree_view, TVM_GETNEXTITEM, TVGN_CARET, 0; 0) as HTREEITEM;

		let mut selected_item: TVITEMEXW = unsafe { mem::zeroed() };
		selected_item.mask = TVIF_PARAM | TVIF_HANDLE;
		selected_item.hItem = selected_item_handle;
		try_send_message!(self.branches_tree_view, TVM_GETITEMW, 0, &mut selected_item as *mut _ as LPARAM; 0);

		let string_ptr = selected_item.lParam as *const String;
		if string_ptr.is_null() { return Ok(false); }

		let ref_name= unsafe { (*string_ptr).as_str() };
		self.model.as_ref().unwrap().switch_to_branch(ref_name);
		Ok(true)
	}

	fn on_commit_right_click(&self, info: &NMITEMACTIVATE) -> bool {
		if info.iItem < 0 { return false; }

		let POINT { x, y } = {
			let mut point = info.ptAction;
			unsafe {
				MapWindowPoints(self.commits_list_view, null_mut(), &mut point as *mut _, 1);
			}
			point
		};

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
				let commits: Vec<Commit> = self.commits[0..info.iItem as usize + 1]
						.iter()
						.map(|commit| commit.clone())
						.collect();
				self.model.as_ref().unwrap().import_commits(commits);
				true
			}
			self::ID_MENU_APPLY => {
				let commit = self.commits[info.iItem as usize].clone();
				self.model.as_ref().unwrap().apply_patches(commit);
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
			bottom: 0,
		};
		try_call!(AdjustWindowRectExForDpi(&mut rect as *mut _ as *mut _, style, TRUE, extended_style, dpi), 0);

		let hor_diff = rect.right - rect.left;
		let client_area_width = width - hor_diff;
		let vert_diff = rect.bottom - rect.top;
		let client_area_height = height - vert_diff;

		try_call!(SetWindowPos(self.branches_tree_view, null_mut(), 0, 0, MainView::BRANCHES_WIDTH, client_area_height - 2 * MainView::EDGE_MARGIN - MainView::LABEL_HEIGHT, SWP_NOMOVE), 0);

		let patches_height = (client_area_height - 2 * (MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT)) / 2;
		let commits_and_patches_hor_pos = client_area_width - MainView::COMMIT_AND_PATCH_HOR_POSITION - MainView::EDGE_MARGIN;
		let commits_vert_pos = MainView::EDGE_MARGIN + 2 * MainView::LABEL_HEIGHT + patches_height;
		try_call!(SetWindowPos(self.combined_patches_list_box, null_mut(), 0, 0, commits_and_patches_hor_pos, patches_height, SWP_NOMOVE), 0);
		try_call!(SetWindowPos(self.commits_list_view, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos, commits_and_patches_hor_pos,
				client_area_height - commits_vert_pos - MainView::EDGE_MARGIN, 0), 0);
		try_call!(SetWindowPos(self.commits_label, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos - MainView::LABEL_HEIGHT,
				MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT, 0), 0);
		Ok(())
	}
}
