use std::ptr::null_mut;
use std::sync::Arc;
use std::mem;
use std::borrow::Cow;

use failure;
use winapi::Interface;
use winapi::ctypes::c_int;
use winapi::shared::basetsd::{LONG_PTR, INT_PTR};
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{DWORD, HINSTANCE, LOWORD, LPARAM, LRESULT, UINT, WORD, WPARAM, TRUE, FALSE};
use winapi::shared::windef::{HBRUSH, HMENU, HWND, POINT, RECT};
use winapi::shared::winerror::S_OK;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::CoCreateInstance;
use winapi::um::shobjidl::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, IFileDialog};
use winapi::um::shobjidl_core::{IShellItem, SIGDN_FILESYSPATH};
use winapi::um::winnt::WCHAR;
use winapi::um::winuser::{self, AdjustWindowRectExForDpi, GetWindowLongW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
	IDC_ARROW, IDI_APPLICATION, GWL_STYLE, GWL_EXSTYLE, SWP_NOMOVE, DialogBoxParamW, LoadAcceleratorsW, LoadCursorW, LoadIconW, MSG, PostQuitMessage,
	PostMessageW, RegisterClassW, ShowWindow, SetWindowPos, SW_SHOWDEFAULT, TranslateAcceleratorW, TranslateMessage, TRACKMOUSEEVENT, WM_APP,
	WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_CHILD, WS_BORDER, WS_TABSTOP, WS_VSCROLL, WS_CLIPCHILDREN, SetWindowTextW, LPNMHDR, WNDPROC,
	FillRect, GWLP_WNDPROC, InvalidateRect, MapWindowPoints, MK_LBUTTON, SetWindowLongPtrW, TME_LEAVE, BS_PUSHBUTTON, SW_HIDE, SW_SHOW,
	TrackMouseEvent};
use winapi::um::commctrl::{self, WC_TREEVIEW, WC_STATIC, TVS_HASLINES, TVM_INSERTITEMW, TVINSERTSTRUCTW, TVI_SORT, TVIF_TEXT,
	TVM_DELETEITEM, TVI_ROOT, TVIF_CHILDREN, HTREEITEM, TVIF_STATE, TVIS_BOLD, TVS_HASBUTTONS, TVS_LINESATROOT, TVIS_EXPANDED, TVM_GETNEXTITEM,
	TVGN_CARET, TVIF_PARAM, TVITEMEXW, TVM_GETITEMW, TVIF_HANDLE, NMLVDISPINFOW, NMITEMACTIVATE, WC_LISTVIEW, LVM_DELETEALLITEMS,
	LVS_REPORT, NMLISTVIEW, LVHITTESTINFO, LVIR_BOUNDS, LVIR_SELECTBOUNDS, LVM_GETCOLUMNWIDTH, LVM_GETITEMCOUNT, LVM_GETITEMRECT,
	LVM_GETNEXTITEM, LVM_HITTEST, LVNI_SELECTED, LVS_EX_DOUBLEBUFFER};
use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};

use super::helpers::*;
use begitter::model::main::{BranchItem, MainModel, MainViewReceiver};
use begitter::change_set::{Commit, ChangeSetInfo};
use begitter::model::View;
use ui::windows::text::{load_string, STRING_MAIN_PATCHES_COLUMNS, STRING_MAIN_WINDOW_NAME, STRING_MAIN_BRANCHES, STRING_MAIN_PATCHES,
	STRING_MAIN_COMMITS, STRING_MAIN_COMMITS_COLUMNS, format_time, STRING_MAIN_ABORT, STRING_MAIN_RESOLVE_REJECTS};
use ui::windows::utils::{set_fonts, get_window_position, insert_columns_into_list_view, insert_rows_into_list_view, close_dialog,
	get_dialog_field_text, get_window_client_area, set_dialog_field_text, show_context_menu};
use ui::windows::dpi::GetDpiForWindow;
use ui::windows::rejects::RejectsView;

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MAIN_MENU_COMMIT: &str = "main_commit_menu";
const MAIN_MENU_COMBINED_PATCH: &str = "main_combined_patch_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;
const ID_MENU_IMPORT: WORD = 200;
const ID_MENU_APPLY: WORD = 201;
const ID_MENU_EDIT_MESSAGE: WORD = 300;
const ID_MENU_DELETE: WORD = 301;

const ID_DIALOG_EDIT_MESSAGE_FIELD: WORD = 1;
const ID_DIALOG_EDIT_MESSAGE_BUTTON_OK: WORD = 2;
const ID_DIALOG_EDIT_MESSAGE_BUTTON_CANCEL: WORD = 3;

const MESSAGE_MODEL_TO_MAIN_VIEW: UINT = WM_APP;

const GUID_FILE_DIALOG: GUID = GUID {
	Data1: 0xdc1c5a9c,
	Data2: 0xe88a,
	Data3: 0x4dde,
	Data4: [0xa5, 0xa1, 0x60, 0xf8, 0x2a, 0x20, 0xae, 0xf7],
};

static mut MAIN_VIEW: Option<MainView> = None;
static mut MAIN_VIEW_RELAY: Option<Arc<MainViewRelay>> = None;

static mut DEFAULT_COMBINED_PATCHES_LIST_VIEW_PROC: WNDPROC = None;

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
	if message == winuser::WM_DESTROY {
		unsafe {
			PostQuitMessage(0);
		}
		return 0;
	}

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

	if handled {
		return 0;
	}

	unsafe {
		DefWindowProcW(h_wnd, message, w_param, l_param)
	}
}

pub extern "system" fn combined_patch_list_view_proc(h_wnd: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
	let message_data = MessageData {
		h_wnd,
		message,
		w_param,
		l_param,
	};

	let handled= unsafe { MAIN_VIEW.as_mut() }
			.map(|main_view| {
				main_view.combined_patches_list_view_drag_tracker
						.pre_message_processing(&message_data)
						.unwrap()
			})
			.unwrap_or(false);

	let result = if handled {
		0 as LRESULT
	} else {
		unsafe {
			DEFAULT_COMBINED_PATCHES_LIST_VIEW_PROC.unwrap()(h_wnd, message, w_param, l_param)
		}
	};

	unsafe { MAIN_VIEW.as_mut() }.map(|main_view| {
		main_view.combined_patches_list_view_drag_tracker.post_message_processing(&message_data)
	});

	result
}

pub extern "system" fn edit_message_dialog_proc(hwnd_dlg: HWND, u_msg : UINT, w_param: WPARAM, l_param: LPARAM) -> INT_PTR {
	let handled = match u_msg {
		winuser::WM_INITDIALOG => {
			let original_text = *unsafe { Box::from_raw(l_param as *mut _) };
			set_dialog_field_text(hwnd_dlg, ID_DIALOG_EDIT_MESSAGE_FIELD as c_int, original_text).unwrap();
			true
		}
		winuser::WM_CLOSE => {
			close_dialog(hwnd_dlg, 0).unwrap();
			true
		}
		winuser::WM_COMMAND => {
			match LOWORD(w_param as DWORD) {
				ID_DIALOG_EDIT_MESSAGE_BUTTON_OK => {
					let text = get_dialog_field_text(hwnd_dlg, ID_DIALOG_EDIT_MESSAGE_FIELD as c_int).unwrap();
					let text_box = Box::into_raw(Box::new(text));
					close_dialog(hwnd_dlg, text_box as INT_PTR).unwrap();
					true
				}
				ID_DIALOG_EDIT_MESSAGE_BUTTON_CANCEL => {
					close_dialog(hwnd_dlg, 0).unwrap();
					true
				}
				_ => false
			}
		}
		_ => false
	};

	(if handled { TRUE } else { FALSE }) as INT_PTR
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

impl View for MainViewRelay {
	fn error(&self, error: failure::Error) {
		println!("We've got an error: {}\n{}", error, error.backtrace()); // TODO: this is not proper error handling
	}
}

impl MainViewReceiver for MainViewRelay {
	fn show_branches(&self, branches: Vec<BranchItem>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Branches(branches)).map_err(|err| err.into())
	}

	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::Commits(commits)).map_err(|err| err.into())
	}

	fn show_combined_patches(&self, combined_patches: Vec<ChangeSetInfo>) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::CombinedPatches(combined_patches)).map_err(|err| err.into())
	}

	fn resolve_rejects(&self) -> Result<(), failure::Error> {
		self.post_on_main_thread(MainViewMessage::ResolveRejects).map_err(|err| err.into())
	}
}

enum MainViewMessage {
	Branches(Vec<BranchItem>),
	Commits(Vec<Commit>),
	CombinedPatches(Vec<ChangeSetInfo>),
	ResolveRejects
}

struct MainView {
	model: Option<MainModel>,

	main_window: HWND,
	commits_label: HWND,
	branches_tree_view: HWND,
	commits_list_view: HWND,
	combined_patches_list_view: HWND,
	continue_button: HWND,
	abort_button: HWND,

	branches: Vec<BranchItem>,
	commits: Vec<Commit>,
	commit_strings: Vec<Vec<WideString>>,
	combined_patches: Vec<ChangeSetInfo>,
	combined_patch_strings: Vec<Vec<WideString>>,

	combined_patches_list_view_drag_tracker: ListViewDragTracker
}

impl MainView {
	const LABEL_WIDTH: c_int = 50;
	const LABEL_HEIGHT: c_int = 25;

	const BUTTON_WIDTH: c_int = 100;
	const BUTTON_HEIGHT: c_int = 25;

	const EDGE_MARGIN: c_int = 7;
	const BRANCHES_WIDTH: c_int = 200;
	const SEPARATOR_WIDTH: c_int = 5;

	const COMMIT_AND_PATCH_HOR_POSITION: c_int = MainView::EDGE_MARGIN + MainView::BRANCHES_WIDTH + MainView::SEPARATOR_WIDTH;

	fn initialize(main_window: HWND) -> Result<MainView, WinApiError> {
		let static_class = to_wstring(WC_STATIC);
		let tree_view_class = to_wstring(WC_TREEVIEW);
		let list_view_class = to_wstring(WC_LISTVIEW);
		let button_class = to_wstring("Button");

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

		let combined_patches_list_view = try_get!(CreateWindowExW(LVS_EX_DOUBLEBUFFER, list_view_class.as_ptr(), null_mut(),
				LVS_REPORT | WS_TABSTOP | WS_BORDER | WS_VISIBLE | WS_CHILD | WS_VSCROLL, MainView::COMMIT_AND_PATCH_HOR_POSITION,
				MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		insert_columns_into_list_view(combined_patches_list_view, STRING_MAIN_PATCHES_COLUMNS)?;

		let commits_list_view = try_get!(CreateWindowExW(LVS_EX_DOUBLEBUFFER, list_view_class.as_ptr(), null_mut(), LVS_REPORT | WS_TABSTOP | WS_BORDER | WS_VISIBLE |
				WS_CHILD | WS_VSCROLL, 0, 0, 0, 0, main_window as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));
		insert_columns_into_list_view(commits_list_view, STRING_MAIN_COMMITS_COLUMNS)?;

		let continue_button = try_get!(CreateWindowExW(0, button_class.as_ptr(), null_mut(), WS_CHILD | BS_PUSHBUTTON, 0, 0, 0, 0, main_window as HWND,
				0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(continue_button, load_string(STRING_MAIN_RESOLVE_REJECTS)?.as_ptr()), 0);

		let abort_button = try_get!(CreateWindowExW(0, button_class.as_ptr(), null_mut(), WS_CHILD | BS_PUSHBUTTON, 0, 0, 0, 0, main_window as HWND,
				0 as HMENU, 0 as HINSTANCE, null_mut()));
		try_call!(SetWindowTextW(abort_button, load_string(STRING_MAIN_ABORT)?.as_ptr()), 0);

		let default_proc = try_call!(SetWindowLongPtrW(combined_patches_list_view, GWLP_WNDPROC, combined_patch_list_view_proc as LONG_PTR), 0);
		unsafe {
			DEFAULT_COMBINED_PATCHES_LIST_VIEW_PROC = Some(mem::transmute(default_proc));
		}

		set_fonts(main_window)?;

		let view = MainView {
			model: None,
			main_window,
			commits_label,
			branches_tree_view,
			combined_patches_list_view,
			commits_list_view,
			continue_button,
			abort_button,
			branches: Vec::new(),
			combined_patches: Vec::new(),
			commits: Vec::new(),
			commit_strings: Vec::new(),
			combined_patch_strings: Vec::new(),
			combined_patches_list_view_drag_tracker: ListViewDragTracker::new(combined_patches_list_view)
		};

		let rect = get_window_position(main_window, main_window)?;
		view.reposition_views(rect.right - rect.left, rect.bottom - rect.top)?;

		Ok(view)
	}

	fn set_model(&mut self, model: MainModel) {
		self.model = Some(model.clone());
		self.combined_patches_list_view_drag_tracker.model = Some(model);
	}

	fn receive_model_message_on_main_thread(&mut self, message_data: &MessageData) -> Result<(), WinApiError> {
		debug_assert_eq!(message_data.message, MESSAGE_MODEL_TO_MAIN_VIEW);
		let arguments = unsafe {
			*Box::from_raw(message_data.l_param as *mut _)
		};

		unsafe {
			ShowWindow(self.continue_button, SW_HIDE);
			ShowWindow(self.abort_button, SW_HIDE);
		}

		match arguments {
			MainViewMessage::Branches(branches) => {
				self.branches = branches;

				try_send_message!(self.branches_tree_view, TVM_DELETEITEM, 0, TVI_ROOT as LPARAM; 0);
				self.view_branches_recursively(&self.branches, null_mut())?;
			}
			MainViewMessage::Commits(commits) => {
				self.commits = commits;
				MainView::update_list_view(self.commits_list_view, &self.commits, &mut self.commit_strings,
						|commit| {
							let change_set_info = &commit.info.change_set_info;
							vec![change_set_info.message.as_str().into(),
								change_set_info.author_action.name.as_str().into(),
								format_time(change_set_info.author_action.time).into(),
								commit.hash.as_str().into()]
						})?;
			}
			MainViewMessage::CombinedPatches(combined_patches) => {
				self.combined_patches = combined_patches;
				MainView::update_list_view(self.combined_patches_list_view, &self.combined_patches, &mut self.combined_patch_strings,
						|patch| {
							vec![patch.message.as_str().into(),
								patch.author_action.name.as_str().into(),
								format_time(patch.author_action.time).into()]
						})?;
			}
			MainViewMessage::ResolveRejects => self.resolve_rejects()
		}

		Ok(())
	}

	fn update_list_view<I>(list_view: HWND, item_slice: &[I], item_strings: &mut Vec<Vec<WideString>>,
			string_generator: fn(&I) -> Vec<Cow<str>>) -> Result<(), WinApiError> {
		try_send_message!(list_view, LVM_DELETEALLITEMS, 0, 0);

		item_strings.clear();
		for item in item_slice {
			let strings = string_generator(item)
					.into_iter()
					.map(|string| to_wstring(&*string))
					.collect();
			item_strings.push(strings);
		}

		insert_rows_into_list_view(list_view, item_slice.len())
	}

	fn view_branches_recursively(&self, branches: &Vec<BranchItem>, parent: HTREEITEM) -> Result<(), WinApiError> {
		for branch in branches {
			fn insert_item(branches_tree_view: HWND, name: &mut WideString, ref_name_ptr: *const String, has_children: bool, bold: bool, expanded: bool,
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

	fn resolve_rejects(&self) {
		let model = self.model.as_ref().unwrap();
		let raw_result = RejectsView::show(self.main_window, model.repo_dir().to_path_buf());
		match *unsafe { Box::from_raw(raw_result as *mut Option<Vec<String>>) } {
			Some(updated_files) => model.continue_application(updated_files),
			None => {
				unsafe {
					ShowWindow(self.continue_button, SW_SHOW);
					ShowWindow(self.abort_button, SW_SHOW);
				}
			}
		}
	}

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = self.combined_patches_list_view_drag_tracker.pre_message_processing(message_data)?;
		if handled {
			return Ok(true)
		}

		let handled = match message_data.message {
			winuser::WM_SIZING => {
				let rect = unsafe { *(message_data.l_param as *const RECT) };
				let width = rect.right - rect.left;
				let height = rect.bottom - rect.top;
				self.reposition_views(width, height)?;
				true
			}
			winuser::WM_COMMAND => {
				if message_data.l_param == 0 {
					match LOWORD(message_data.w_param as DWORD) {
						ID_MENU_OPEN => {
							if let Ok(dir) = show_open_file_dialog(self.main_window) {
								self.set_model(MainModel::new(unsafe { MAIN_VIEW_RELAY.as_mut() }.unwrap().clone(), dir));
							}
							true
						}
						_ => false
					}
				} else if message_data.l_param as HWND == self.continue_button {
					self.resolve_rejects();
					true
				} else if message_data.l_param as HWND == self.abort_button {
					self.model.as_ref().unwrap().abort();
					true
				} else {
					false
				}
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
						} else if header.hwndFrom == self.combined_patches_list_view {
							let info = unsafe { (message_data.l_param as *const NMITEMACTIVATE).as_ref().unwrap() };
							self.on_combined_patch_click(&info)?
						} else {
							false
						}
					}
					commctrl::LVN_GETDISPINFOW => {
						fn fill_info(l_param: LPARAM, strings: &mut Vec<Vec<WideString>>) {
							let info = unsafe { (l_param as *mut NMLVDISPINFOW).as_mut().unwrap() };
							info.item.pszText = strings[info.item.iItem as usize][info.item.iSubItem as usize].as_mut_ptr();
						}

						if header.hwndFrom == self.commits_list_view {
							fill_info(message_data.l_param, &mut self.commit_strings);
							true
						} else if header.hwndFrom == self.combined_patches_list_view {
							fill_info(message_data.l_param, &mut self.combined_patch_strings);
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

		let result = show_context_menu(self.main_window, self.commits_list_view, &info.ptAction, MAIN_MENU_COMMIT);
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

	fn on_combined_patch_click(&self, info: &NMITEMACTIVATE) -> Result<bool, WinApiError> {
		if info.iItem < 0 { return Ok(false); }

		let selected_item = try_send_message!(self.combined_patches_list_view, LVM_GETNEXTITEM, -1isize as usize, LVNI_SELECTED);
		if selected_item < 0 {
			panic!("Nothing is selected in the combined patch list view");
		}

		let result = show_context_menu(self.main_window, self.combined_patches_list_view, &info.ptAction, MAIN_MENU_COMBINED_PATCH);
		match result {
			self::ID_MENU_EDIT_MESSAGE => {
				let original_text = Box::into_raw(Box::new(self.combined_patches[selected_item as usize].message.clone()));
				let text_box_ptr = unsafe { DialogBoxParamW(null_mut(), to_wstring("main_commit_message_dialog").as_ptr(),
					self.main_window, Some(edit_message_dialog_proc), original_text as LPARAM) };
				let mut text = match text_box_ptr {
					0 => return Ok(false),
					_ => {
						*unsafe { Box::from_raw(text_box_ptr as *mut WideString) }
					}
				};

				self.model.as_ref().unwrap().set_patch_message(selected_item as usize, from_wstring(text.as_mut_ptr()));
			},
			self::ID_MENU_DELETE => {
				self.model.as_ref().unwrap().delete(selected_item as usize);
			},
			_ => return Ok(false)
		}

		Ok(true)
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

		try_call!(SetWindowPos(self.branches_tree_view, null_mut(), 0, 0, MainView::BRANCHES_WIDTH, client_area_height - 2 * MainView::EDGE_MARGIN -
				MainView::LABEL_HEIGHT, SWP_NOMOVE), 0);

		let patches_height = (client_area_height - 2 * (MainView::EDGE_MARGIN + MainView::LABEL_HEIGHT)) / 2;
		let commits_and_patches_hor_pos = client_area_width - MainView::COMMIT_AND_PATCH_HOR_POSITION - MainView::EDGE_MARGIN;
		let commits_vert_pos = MainView::EDGE_MARGIN + 2 * MainView::LABEL_HEIGHT + patches_height + 6;
		try_call!(SetWindowPos(self.combined_patches_list_view, null_mut(), 0, 0, commits_and_patches_hor_pos, patches_height, SWP_NOMOVE), 0);
		try_call!(SetWindowPos(self.commits_list_view, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos, commits_and_patches_hor_pos,
				client_area_height - commits_vert_pos - MainView::EDGE_MARGIN, 0), 0);
		try_call!(SetWindowPos(self.commits_label, null_mut(), MainView::COMMIT_AND_PATCH_HOR_POSITION, commits_vert_pos - MainView::LABEL_HEIGHT,
				MainView::LABEL_WIDTH, MainView::LABEL_HEIGHT, 0), 0);
		try_call!(SetWindowPos(self.continue_button, null_mut(), client_area_width - MainView::EDGE_MARGIN - MainView::BUTTON_WIDTH,
				commits_vert_pos - MainView::BUTTON_HEIGHT - 4, MainView::BUTTON_WIDTH, MainView::BUTTON_HEIGHT, 0), 0);
		try_call!(SetWindowPos(self.abort_button, null_mut(), client_area_width - MainView::EDGE_MARGIN - MainView::SEPARATOR_WIDTH -
				MainView::BUTTON_WIDTH * 2, commits_vert_pos - MainView::BUTTON_HEIGHT - 4, MainView::BUTTON_WIDTH, MainView::BUTTON_HEIGHT, 0), 0);
		Ok(())
	}
}

struct ListViewDragTracker {
	model: Option<MainModel>,
	combined_patches_list_view: HWND,
	item_positions: Option<(usize, usize)>
}

impl ListViewDragTracker {
	fn new<'a>(combined_patches_list_view: HWND) -> ListViewDragTracker {
		ListViewDragTracker {
			model: None,
			combined_patches_list_view,
			item_positions: None
		}
	}

	fn invalidate(&self) -> Result<(), WinApiError> {
		let client_area = get_window_client_area(self.combined_patches_list_view)?;
		try_call!(InvalidateRect(self.combined_patches_list_view, &client_area as *const RECT, TRUE), 0);
		Ok(())
	}

	fn pre_message_processing(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		let handled = match message_data.message {
			winuser::WM_NOTIFY => {
				if unsafe { (*(message_data.l_param as LPNMHDR)).hwndFrom } != self.combined_patches_list_view {
					false
				} else {
					let header = unsafe { *(message_data.l_param as LPNMHDR) };
					match header.code {
						commctrl::LVN_BEGINDRAG => {
							let mut tracking_request: TRACKMOUSEEVENT = unsafe { mem::zeroed() };
							tracking_request.cbSize = mem::size_of_val(&tracking_request) as DWORD;
							tracking_request.dwFlags = TME_LEAVE;
							tracking_request.hwndTrack = self.combined_patches_list_view;
							try_call!(TrackMouseEvent(&mut tracking_request as *mut _ as *mut _), 0);

							let item =  unsafe { *(message_data.l_param as *const NMLISTVIEW) };
							self.item_positions = Some((item.iItem as usize, item.iItem as usize));
							self.invalidate()?;
							true
						}
						_ => false
					}
				}
			}
			winuser::WM_MOUSEMOVE => {
				let item_count = try_send_message!(self.combined_patches_list_view, LVM_GETITEMCOUNT, 0, 0);
				if item_count <= 0 {
					self.item_positions = None;
					return Ok(false)
				}

				let new_insertion_position = match self.item_positions {
					Some(_) => {
						if message_data.w_param & MK_LBUTTON == 0 {
							None
						} else {
							let x = GET_X_LPARAM(message_data.l_param);
							let y = GET_Y_LPARAM(message_data.l_param);

							let mut hit_test_info: LVHITTESTINFO = unsafe { mem::zeroed() };
							hit_test_info.pt.x = x;
							hit_test_info.pt.y = y;
							try_call!(MapWindowPoints(message_data.h_wnd, self.combined_patches_list_view, &mut hit_test_info.pt as *mut _ as *mut _, 1), 0);

							Some(self.calculate_insertion_position(message_data, item_count as usize)?)
						}
					}
					None => None
				};

				match new_insertion_position {
					None => self.item_positions = None,
					Some(new_position) => {
						if let &mut Some((_, ref mut position)) = &mut self.item_positions {
							*position = new_position;
						} else {
							panic!("Currently not in dragging mode");
						}
						self.invalidate()?;
					}
				}
				false
			}
			winuser::WM_LBUTTONUP => {
				if let &Some((source_position, _)) = &self.item_positions {
					let item_count = try_send_message!(self.combined_patches_list_view, LVM_GETITEMCOUNT, 0, 0);
					if item_count > 0 {
						let insertion_position = self.calculate_insertion_position(message_data, item_count as usize)?;
						self.model.as_ref().unwrap().move_patch(source_position, insertion_position);

						self.item_positions = None;
						self.invalidate()?;
					}
				}
				false
			}
			winuser::WM_MOUSELEAVE => {
				self.item_positions = None;
				true
			}
			_ => false
		};
		Ok(handled)
	}

	fn post_message_processing(&mut self, message_data: &MessageData) -> Result<(), WinApiError> {
		if message_data.message != winuser::WM_PAINT {
			return Ok(());
		}

		let item_count = try_send_message!(self.combined_patches_list_view, LVM_GETITEMCOUNT, 0, 0);
		if item_count <= 0 {
			self.item_positions = None;
			return Ok(())
		}
		let item_count = item_count as usize;

		let insertion_position = match self.item_positions {
			Some((_, insertion_position)) => insertion_position,
			None => return Ok(())
		};

		let mut rect: RECT = unsafe { mem::zeroed() };
		rect.left = LVIR_SELECTBOUNDS;
		let index = if insertion_position >= item_count { insertion_position - 1} else { insertion_position };
		try_send_message!(self.combined_patches_list_view, LVM_GETITEMRECT, index, &mut rect as *mut _ as LPARAM);

		let insertion_mark_vert_center = if insertion_position >= item_count { rect.bottom } else { rect.top };
		let column_width = try_send_message!(self.combined_patches_list_view, LVM_GETCOLUMNWIDTH, 0, 0) as c_int;
		let insertion_mark_rect = RECT {
			left: rect.left,
			right: rect.left + column_width,
			top: insertion_mark_vert_center - 1,
			bottom: insertion_mark_vert_center + 1
		};

		let data_holder = PaintingDataHolder::new(self.combined_patches_list_view)?;
		let brush = Brush::new_solid(0xFF, 0, 0)?;
		try_call!(FillRect(data_holder.context(), &insertion_mark_rect, brush.brush()), 0);

		Ok(())
	}

	fn calculate_insertion_position(&self, message_data: &MessageData, item_count: usize) -> Result<usize, WinApiError> {
		let x = GET_X_LPARAM(message_data.l_param);
		let y = GET_Y_LPARAM(message_data.l_param);

		let mut hit_test_info: LVHITTESTINFO = unsafe { mem::zeroed() };
		hit_test_info.pt.x = x;
		hit_test_info.pt.y = y;
		try_call!(MapWindowPoints(message_data.h_wnd, self.combined_patches_list_view, &mut hit_test_info.pt as *mut _ as *mut _, 1), 0);

		let index = try_send_message!(self.combined_patches_list_view, LVM_HITTEST, 0, &mut hit_test_info as *mut _ as LPARAM);
		let insertion_point = if index >= 0 {
			index as usize
		} else {
			let mut bounds: RECT = unsafe { mem::zeroed() };
			bounds.left = LVIR_BOUNDS;
			try_send_message!(self.combined_patches_list_view, LVM_GETITEMRECT, 0, &mut bounds as *mut _ as LPARAM; 0);

			if y < bounds.top {
				0
			} else {
				item_count
			}
		};
		Ok(insertion_point)
	}
}
