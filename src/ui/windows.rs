use winapi::shared::ntdef::HRESULT;
use winapi::shared::minwindef::{LRESULT, HINSTANCE, UINT, WPARAM, LPARAM, DWORD, LOWORD, WORD};
use winapi::shared::windef::{POINT, HBRUSH, HMENU, HWND};
use winapi::shared::guiddef::GUID;
use winapi::shared::winerror::S_OK;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::Interface;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::combaseapi::CoTaskMemFree;
use winapi::um::shobjidl::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, IFileDialog};
use winapi::um::shobjidl_core::{IShellItem, SIGDN_FILESYSPATH};
use winapi::um::combaseapi::CoCreateInstance;
use winapi::um::unknwnbase::IUnknown;
use winapi::um::winnt::{PWSTR, WCHAR};
use winapi::um::winuser::{SW_SHOWNORMAL, PostQuitMessage, LoadAcceleratorsW, TranslateAcceleratorW, MSG, WS_VISIBLE, WS_OVERLAPPEDWINDOW, WNDCLASSW, GetMessageW, TranslateMessage, DispatchMessageW, RegisterClassW, ShowWindow, DefWindowProcW, LoadIconW, LoadCursorW, IDI_APPLICATION, IDC_ARROW, CreateWindowExW};
use libc::wcslen;

use std::mem;
use std::ffi::{OsStr, OsString};
use std::ops::{Deref, DerefMut};
use std::os::windows::ffi::{OsStringExt, OsStrExt};
use std::ptr::null_mut;
use std::slice;

use super::text::WINDOW_NAME;
use winapi::um::winuser;

macro_rules! try_com {
	($($call:ident).+($($args:tt)*)) => {
		try_com!(@parse () () () ($($call).*) () ($($args)*));
	};
	(@parse ($($out_vars:ident)*) $com_out_vars:tt $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) (out $arg:ident)) => {
		try_com!(@parse ($($out_vars)* $arg) $com_out_vars $com_mem_out_vars $call ($($parsed_args)* &mut $arg as *mut _,) ());
	};
	(@parse ($($out_vars:ident)*) $com_out_vars:tt $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) (out $arg:ident, $($rest:tt)*)) => {
		try_com!(@parse ($($out_vars)* $arg) $com_out_vars $com_mem_out_vars $call ($($parsed_args)* &mut $arg as *mut _,) ($($rest)*));
	};
	(@parse $out_vars:tt ($($com_out_vars:ident)*) $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) (com_out $arg:ident: $type:ty)) => {
		try_com!(@parse $out_vars ($($com_out_vars)* $arg) $com_mem_out_vars $call ($($parsed_args)* ComPtr::<$type>::as_out_param(&mut $arg),) ());
	};
	(@parse $out_vars:tt ($($com_out_vars:ident)*) $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) (com_out $arg:ident: $type:ty, $($rest:tt)*)) => {
		try_com!(@parse $out_vars ($($com_out_vars)* $arg) $com_mem_out_vars $call ($($parsed_args)* ComPtr::<$type>::as_out_param(&mut $arg),) ($($rest)*));
	};
	(@parse $out_vars:tt $com_out_vars:tt ($($com_mem_out_vars:ident)*) $call:tt ($($parsed_args:tt)*) (com_mem_out $arg:ident: $type:ty)) => {
		try_com!(@parse $out_vars $com_out_vars ($($com_mem_out_vars)* $arg) $call ($($parsed_args)* ComMemPtr::<$type>::as_out_param(&mut $arg),) ());
	};
	(@parse $out_vars:tt $com_out_vars:tt ($($com_mem_out_vars:ident)*) $call:tt ($($parsed_args:tt)*) (com_mem_out $arg:ident: $type:ty, $($rest:tt)*)) => {
		try_com!(@parse $out_vars $com_out_vars ($($com_mem_out_vars)* $arg) $call ($($parsed_args)* ComMemPtr::<$type>::as_out_param(&mut $arg),) ($($rest)*));
	};
	(@parse $out_vars:tt $com_out_vars:tt $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) ($arg:expr)) => {
		try_com!(@parse $out_vars $com_out_vars $com_mem_out_vars $call ($($parsed_args)* $arg,) ());
	};
	(@parse $out_vars:tt $com_out_vars:tt $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) ($arg:expr, $($rest:tt)*)) => {
		try_com!(@parse $out_vars $com_out_vars $com_mem_out_vars $call ($($parsed_args)* $arg,) ($($rest)*));
	};
	(@parse ($($out_vars:ident)*) ($($com_out_vars:ident)*) ($($com_mem_out_vars:ident)*) ($($call:tt)*) ($($parsed_args:tt)*) ()) => {
		$(
			let mut $out_vars = unsafe {
				mem::uninitialized()
			};
		)*
		$(
			let mut $com_out_vars = ComPtr::new();
		)*
		$(
			let mut $com_mem_out_vars = ComMemPtr::new();
		)*
		unsafe {
			let result = $($call)*($($parsed_args)*);
			if result != S_OK {
				return ::std::result::Result::Err(result)
			}
		}
	};
}

macro_rules! try_get {
	($($call:ident).+($($args:expr),*)) => {
		unsafe {
			let result = $($call).*($($args),*);
			if result.is_null() {
				return ::std::result::Result::Err(GetLastError());
			}
			result
		}
	};
}

macro_rules! try_call {
	($($call:ident).+($($args:expr),*)) => {
		unsafe {
			let result = $($call).*($($args),*);
			if result == 0 {
				return ::std::result::Result::Err(GetLastError());
			}
			result
		}
	};
}

const MAIN_CLASS: &str = "main";

const MAIN_MENU: &str = "main_menu";
const MAIN_ACCELERATORS: &str = "main_accelerators";

const ID_MENU_OPEN: WORD = 100;

const GUID_FILE_DIALOG: GUID = GUID {
	Data1: 0xdc1c5a9c,
	Data2: 0xe88a,
	Data3: 0x4dde,
	Data4: [0xa5, 0xa1, 0x60, 0xf8, 0x2a, 0x20, 0xae, 0xf7],
};

pub fn run() -> Result<(), u32> {
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

	try_call!(RegisterClassW(&wnd));

	let h_wnd_window = try_get!(CreateWindowExW(0, class_name.as_ptr(), to_wstring(WINDOW_NAME).as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
		0, 0, 500, 500, 0 as HWND, 0 as HMENU, 0 as HINSTANCE, null_mut()));

	unsafe {
		ShowWindow(h_wnd_window, SW_SHOWNORMAL);
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
		try_call!(GetMessageW(&mut msg, 0 as HWND, 0, 0));

		match msg.message {
			winuser::WM_QUIT => break,
			_ => {
				unsafe {
					if TranslateAcceleratorW(h_wnd_window, accelerators, &mut msg) == 0 {
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
					show_open_file_dialog(h_wnd);
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

fn show_open_file_dialog(owner: HWND) -> Result<(), HRESULT> {
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
	println!("Got display name: {}", from_wstring(&mut *display_name as *mut _));

	Ok(())
}

fn to_wstring(string: &str) -> Vec<u16> {
	let mut data: Vec<u16> = OsStr::new(string).encode_wide().collect();
	data.push(0);
	data
}

fn from_wstring(string: PWSTR) -> String {
	let string_slice = unsafe {
		let mut len = wcslen(string);
		slice::from_raw_parts(string, len)
	};

	OsString::from_wide(string_slice).into_string().unwrap()
}

struct ComPtr<T: Interface> {
	ptr: *mut T
}

impl<T: Interface> ComPtr<T> {
	fn new() -> ComPtr<T> {
		ComPtr {
			ptr: null_mut()
		}
	}

	fn as_out_param<O>(&mut self) -> *mut *mut O {
		&mut self.ptr as *mut _ as *mut _
	}
}

impl<T: Interface> Drop for ComPtr<T> {
	fn drop(&mut self) {
		unsafe {
			if let Some(ptr) = (self.ptr as *const IUnknown).as_ref() {
				ptr.Release();
			}
		}
	}
}

impl<T: Interface> Deref for ComPtr<T> {
	type Target = T;

	fn deref(&self) -> &T {
		unsafe {
			self.ptr.as_ref().unwrap()
		}
	}
}

impl<T: Interface> DerefMut for ComPtr<T> {
	fn deref_mut(&mut self) -> &mut T {
		unsafe {
			self.ptr.as_mut().unwrap()
		}
	}
}

struct ComMemPtr<T> {
	ptr: *mut T
}

impl<T> ComMemPtr<T> {
	fn new() -> ComMemPtr<T> {
		ComMemPtr {
			ptr: null_mut()
		}
	}

	fn as_out_param(&mut self) -> *mut *mut T {
		&mut self.ptr as *mut _ as *mut _
	}
}

impl<T> Drop for ComMemPtr<T> {
	fn drop(&mut self) {
		unsafe {
			if let Some(ptr) = self.ptr.as_mut() {
				CoTaskMemFree(ptr as *mut _ as *mut _);
			}
		}
	}
}

impl<T> Deref for ComMemPtr<T> {
	type Target = T;

	fn deref(&self) -> &T {
		unsafe {
			self.ptr.as_ref().unwrap()
		}
	}
}

impl<T> DerefMut for ComMemPtr<T> {
	fn deref_mut(&mut self) -> &mut T {
		unsafe {
			self.ptr.as_mut().unwrap()
		}
	}
}