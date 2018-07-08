use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::Arc;

use failure;

use ui::windows::helpers::{MessageData, to_wstring, WinApiError};
use ui::windows::utils::close_dialog;
use winapi::um::winuser::{self, DialogBoxParamW, GetDlgItem, LB_ADDSTRING, LB_ERR, LB_ERRSPACE, LB_RESETCONTENT, PostMessageW, SetWindowTextW, WM_APP,
	LB_SETCURSEL, LBN_SELCHANGE, LB_GETCURSEL, GetWindowTextW, GetWindowTextLengthW, EnableWindow, BN_CLICKED};
use winapi::shared::minwindef::{FALSE, LPARAM, TRUE, UINT, WPARAM, LOWORD, DWORD, HIWORD};
use winapi::ctypes::c_int;
use winapi::shared::{basetsd::INT_PTR, windef::HWND, ntdef::PWSTR, ntdef::WCHAR};

use begitter::model::rejects::{RejectsModel, RejectsViewReceiver};
use begitter::model::View;
use begitter::patch_editor::patch::Hunk;
use ui::windows::helpers::from_wstring;
use ui::windows::text::{load_string, STRING_REJECTS_UNACCEPT_HUNK, STRING_REJECTS_ACCEPT_HUNK};

const ID_REJECTS_FILES_LISTBOX: c_int = 3;
const ID_REJECTS_HUNKS_LISTBOX: c_int = 4;
const ID_REJECTS_RESET_BUTTON: c_int = 5;
const ID_REJECTS_ACCEPT_BUTTON: c_int = 6;
const ID_REJECTS_FILE_EDIT_TEXT: c_int = 7;
const ID_REJECTS_HUNK_EDIT_TEXT: c_int = 8;
const ID_REJECTS_SAVE_AND_QUIT_BUTTON: c_int = 9;
const ID_REJECTS_ABORT_BUTTON: c_int = 10;

const MESSAGE_MODEL_TO_REJECTS_VIEW: UINT = WM_APP + 1;

static mut REJECTS_VIEW: Option<RejectsView> = None;

enum RejectsViewMessage {
	ShowFiles(Vec<(String, bool)>),
	ShowHunks(Vec<(Arc<Hunk>, bool)>),
	ShowFileData(Arc<Vec<u8>>, usize),
	ShowActiveHunk(Arc<Hunk>, usize),
	Finish
}

pub struct RejectsView {
	model: RejectsModel,

	rejects_window: HWND,
	files_list_box: HWND,
	hunks_list_box: HWND,
	accept_button: HWND,
	file_edit_text: HWND,
	hunk_edit_text: HWND,
	save_and_quit_button: HWND,

	files: Vec<(String, bool)>, // TODO: but do we need it as a field? Same for the main view, actually
	hunks: Vec<(Arc<Hunk>, bool)>,
}

impl RejectsView {
	pub fn show(parent: HWND, repo_path: PathBuf) -> INT_PTR {
		unsafe {
			DialogBoxParamW(null_mut(), to_wstring("rejects_dialog").as_ptr(),
				parent, Some(rejects_dialog_proc), Box::into_raw(Box::new(repo_path)) as LPARAM)
		}
	}

	fn initialize(model: RejectsModel, rejects_window: HWND) -> Result<RejectsView, WinApiError> {
		Ok(RejectsView {
			model,
			rejects_window,
			files_list_box: try_get!(GetDlgItem(rejects_window, ID_REJECTS_FILES_LISTBOX)),
			hunks_list_box: try_get!(GetDlgItem(rejects_window, ID_REJECTS_HUNKS_LISTBOX)),
			accept_button: try_get!(GetDlgItem(rejects_window, ID_REJECTS_ACCEPT_BUTTON)),
			file_edit_text: try_get!(GetDlgItem(rejects_window, ID_REJECTS_FILE_EDIT_TEXT)),
			hunk_edit_text: try_get!(GetDlgItem(rejects_window, ID_REJECTS_HUNK_EDIT_TEXT)),
			save_and_quit_button: try_get!(GetDlgItem(rejects_window, ID_REJECTS_SAVE_AND_QUIT_BUTTON)),
			files: Vec::new(),
			hunks: Vec::new(),
		})
	}

	fn receive_message(&mut self, message_data: &MessageData) -> Result<bool, WinApiError> {
		Ok(match message_data.message {
			winuser::WM_COMMAND if HIWORD(message_data.w_param as DWORD) == LBN_SELCHANGE => {
				match LOWORD(message_data.w_param as DWORD) as c_int {
					ID_REJECTS_FILES_LISTBOX => {
						let new_file = try_send_message!(self.files_list_box, LB_GETCURSEL, 0, 0);
						if new_file < 0 {
							false
						} else {
							self.save_changes_to_current_file()?;
							self.model.switch_to_file(new_file as usize);
							true
						}
					}
					ID_REJECTS_HUNKS_LISTBOX => {
						let new_hunk = try_send_message!(self.hunks_list_box, LB_GETCURSEL, 0, 0);
						if new_hunk < 0 {
							false
						} else {
							self.model.switch_to_hunk(new_hunk as usize);
							true
						}
					}
					_ => false
				}
			}
			winuser::WM_COMMAND if HIWORD(message_data.w_param as DWORD) == BN_CLICKED => {
				match LOWORD(message_data.w_param as DWORD) as c_int {
					ID_REJECTS_ACCEPT_BUTTON => {
						let file_pos = try_send_message!(self.files_list_box, LB_GETCURSEL, 0, 0);
						let hunk_pos = try_send_message!(self.hunks_list_box, LB_GETCURSEL, 0, 0);
						if file_pos >= 0 && hunk_pos >= 0 {
							self.model.set_hunk_accepted(file_pos as usize, hunk_pos as usize, !self.hunks[hunk_pos as usize].1);
						}
						true
					}
					ID_REJECTS_RESET_BUTTON => {
						let file_pos = try_send_message!(self.files_list_box, LB_GETCURSEL, 0, 0);
						if file_pos >= 0 {
							self.model.reset(file_pos as usize);
						}
						true
					}
					ID_REJECTS_SAVE_AND_QUIT_BUTTON => {
						self.save_changes_to_current_file()?;
						self.model.save_and_quit();
						true
					}
					ID_REJECTS_ABORT_BUTTON => {
						let result = Box::into_raw(Box::new(None::<Vec<String>>));
						close_dialog(self.rejects_window, result as INT_PTR)?;
						true
					}
					_ => false
				}
			}
			self::MESSAGE_MODEL_TO_REJECTS_VIEW => {
				self.receive_model_message_on_main_thread(message_data)?;
				true
			}
			_ => false
		})
	}

	fn receive_model_message_on_main_thread(&mut self, message_data: &MessageData) -> Result<(), WinApiError> {
		debug_assert_eq!(message_data.message, MESSAGE_MODEL_TO_REJECTS_VIEW);
		let arguments = unsafe {
			*Box::from_raw(message_data.l_param as *mut _)
		};

		fn set_edit_text_contents(handle: HWND, data: &Vec<u8>) -> Result<(), WinApiError> {
			let mut raw_text = match String::from_utf8(data.clone()) {
				Ok(text) => text,
				Err(err) => {
					println!("Couldn't read the file text: {:?}", err); // TODO: this isn't proper handling
					return Ok(());
				}
			};

			if !raw_text.contains("\r\n") && raw_text.contains("\n") { // TODO: this should be rolled back
				raw_text = raw_text.replace("\n", "\r\n");
			}
			let text = to_wstring(&raw_text);
			try_call!(SetWindowTextW(handle, text.as_ptr() as *const _ as *const _), 0);

			Ok(())
		}

		match arguments {
			RejectsViewMessage::ShowFiles(files) => {
				self.files = files;

				try_send_message!(self.files_list_box, LB_RESETCONTENT, 0, 0);
				for &(ref file, accepted) in &self.files {
					let string = to_wstring(&format!("{} {}", if accepted { " " } else { "!" }, file));
					try_send_message!(self.files_list_box, LB_ADDSTRING, 0, string.as_ptr() as LPARAM; LB_ERR, LB_ERRSPACE);
				}

				unsafe {
					EnableWindow(self.save_and_quit_button, if self.files.iter().all(|&(_, accepted)| accepted) { TRUE } else { FALSE });
				}
			}
			RejectsViewMessage::ShowHunks(hunks) => {
				self.hunks = hunks;

				try_send_message!(self.hunks_list_box, LB_RESETCONTENT, 0, 0);
				for &(ref hunk, accepted) in &self.hunks {
					let string = format!("{} -{},{} +{},{}", if accepted { " " } else { "!" }, hunk.old_file_range.start,
						hunk.old_file_range.end - hunk.old_file_range.start, hunk.new_file_range.start,
						hunk.new_file_range.end - hunk.new_file_range.start);
					let wide_string = to_wstring(&string);

					try_send_message!(self.hunks_list_box, LB_ADDSTRING, 0, wide_string.as_ptr() as LPARAM; LB_ERR, LB_ERRSPACE);
				}
			}
			RejectsViewMessage::ShowFileData(data, file_pos) => {
				try_send_message!(self.files_list_box, LB_SETCURSEL, file_pos, 0);
				set_edit_text_contents(self.file_edit_text, &*data)?;
			}
			RejectsViewMessage::ShowActiveHunk(hunk, hunk_pos) => {
				try_send_message!(self.hunks_list_box, LB_SETCURSEL, hunk_pos, 0);
				set_edit_text_contents(self.hunk_edit_text, &hunk.data)?;

				let string_id = if self.hunks[hunk_pos].1 { STRING_REJECTS_UNACCEPT_HUNK } else { STRING_REJECTS_ACCEPT_HUNK };
				let accept_button_text = load_string(string_id)?;

				try_call!(SetWindowTextW(self.accept_button, accept_button_text.as_ptr() as *const _ as *const _), 0);
			}
			RejectsViewMessage::Finish => {
				let files = self.files.iter().map(|&(ref file, _)| file.clone()).collect::<Vec<String>>();
				let result = Box::into_raw(Box::new(files));
				close_dialog(self.rejects_window, result as INT_PTR)?;
			}
		}

		Ok(())
	}

	fn save_changes_to_current_file(&self) -> Result<(), WinApiError> {
		let new_file = try_send_message!(self.files_list_box, LB_GETCURSEL, 0, 0);
		if new_file < 0 {
			return Ok(());
		}

		let text_length = try_call!(GetWindowTextLengthW(self.file_edit_text), 0) + 1;
		let mut text_buf = vec![0 as WCHAR; text_length as usize];
		try_call!(GetWindowTextW(self.file_edit_text, text_buf.as_mut_ptr(), text_length), 0);
		self.model.update_changes_to_current_file(from_wstring(text_buf.as_mut_ptr() as PWSTR).into_bytes());

		Ok(())
	}
}

pub extern "system" fn rejects_dialog_proc(hwnd_dlg: HWND, message: UINT, w_param: WPARAM, l_param: LPARAM) -> INT_PTR {
	let handled = match message {
		winuser::WM_INITDIALOG => {
			let repo_path = *unsafe { Box::from_raw(l_param as *mut PathBuf) };
			let relay = Arc::new(RejectsViewRelay {
				rejects_window: hwnd_dlg
			});
			let model = RejectsModel::new(relay, repo_path);

			unsafe {
				REJECTS_VIEW = Some(RejectsView::initialize(model, hwnd_dlg).unwrap());
				REJECTS_VIEW.as_ref().unwrap().model.scan_files();
			}
			true
		}
		winuser::WM_CLOSE => {
			let result = Box::into_raw(Box::new(None::<Vec<String>>));
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
			match unsafe { REJECTS_VIEW.as_mut() } {
				Some(ref mut view) => view.receive_message(message_data).unwrap(),
				None => false
			}
		}
	};

	(if handled { TRUE } else { FALSE }) as INT_PTR
}

struct RejectsViewRelay {
	rejects_window: HWND
}

unsafe impl Send for RejectsViewRelay {}

unsafe impl Sync for RejectsViewRelay {}

impl RejectsViewRelay {
	fn post_on_main_thread(&self, message: RejectsViewMessage) -> Result<(), WinApiError> {
		let message = Box::new(message);
		try_call!(PostMessageW(self.rejects_window, MESSAGE_MODEL_TO_REJECTS_VIEW, 0, Box::into_raw(message) as LPARAM), 0);
		Ok(())
	}
}

impl View for RejectsViewRelay {
	fn error(&self, error: failure::Error) {
		println!("{}", error);
	}
}

impl RejectsViewReceiver for RejectsViewRelay {
	fn show_files(&self, files: Vec<(String, bool)>) {
		self.post_on_main_thread(RejectsViewMessage::ShowFiles(files)).unwrap();
	}

	fn show_file_hunks(&self, hunks: Vec<(Arc<Hunk>, bool)>) {
		self.post_on_main_thread(RejectsViewMessage::ShowHunks(hunks)).unwrap();
	}

	fn show_file_data(&self, data: Arc<Vec<u8>>, file_pos: usize) {
		self.post_on_main_thread(RejectsViewMessage::ShowFileData(data, file_pos)).unwrap();
	}

	fn show_active_hunk(&self, hunk: Arc<Hunk>, hunk_pos: usize) {
		self.post_on_main_thread(RejectsViewMessage::ShowActiveHunk(hunk, hunk_pos)).unwrap();
	}

	fn finish(&self) {
		self.post_on_main_thread(RejectsViewMessage::Finish).unwrap();
	}
}

