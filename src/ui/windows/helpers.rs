use std::ffi::{OsStr, OsString};
use std::ops::{Deref, DerefMut};
use std::os::windows::ffi::{OsStringExt, OsStrExt};
use std::ptr::null_mut;
use std::slice;
use std::result::Result::Ok;

use winapi::Interface;
use winapi::shared::ntdef::PWSTR;
use winapi::shared::minwindef::HINSTANCE;
use winapi::shared::windef::HMENU;
use winapi::um::combaseapi::CoTaskMemFree;
use winapi::um::unknwnbase::IUnknown;
use winapi::um::winuser::{LoadMenuW, DestroyMenu};
use libc::wcslen;
use failure::Backtrace;

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
		try_com!(@parse $out_vars ($($com_out_vars)* $arg) $com_mem_out_vars $call ($($parsed_args)* ::ui::windows::helpers::ComPtr::<$type>::as_out_param(&mut $arg),) ());
	};
	(@parse $out_vars:tt ($($com_out_vars:ident)*) $com_mem_out_vars:tt $call:tt ($($parsed_args:tt)*) (com_out $arg:ident: $type:ty, $($rest:tt)*)) => {
		try_com!(@parse $out_vars ($($com_out_vars)* $arg) $com_mem_out_vars $call ($($parsed_args)* ::ui::windows::helpers::ComPtr::<$type>::as_out_param(&mut $arg),) ($($rest)*));
	};
	(@parse $out_vars:tt $com_out_vars:tt ($($com_mem_out_vars:ident)*) $call:tt ($($parsed_args:tt)*) (com_mem_out $arg:ident: $type:ty)) => {
		try_com!(@parse $out_vars $com_out_vars ($($com_mem_out_vars)* $arg) $call ($($parsed_args)* ::ui::windows::helpers::ComMemPtr::<$type>::as_out_param(&mut $arg),) ());
	};
	(@parse $out_vars:tt $com_out_vars:tt ($($com_mem_out_vars:ident)*) $call:tt ($($parsed_args:tt)*) (com_mem_out $arg:ident: $type:ty, $($rest:tt)*)) => {
		try_com!(@parse $out_vars $com_out_vars ($($com_mem_out_vars)* $arg) $call ($($parsed_args)* ::ui::windows::helpers::ComMemPtr::<$type>::as_out_param(&mut $arg),) ($($rest)*));
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
				::std::mem::uninitialized()
			};
		)*
		$(
			let mut $com_out_vars = ::ui::windows::helpers::ComPtr::new();
		)*
		$(
			let mut $com_mem_out_vars = ::ui::windows::helpers::ComMemPtr::new();
		)*
		unsafe {
			let result = $($call)*($($parsed_args)*);
			if result != S_OK {
				return ::std::result::Result::Err(::ui::windows::helpers::WinApiError(result as u64, ::failure::Backtrace::new()))
			}
		}
	};
}

macro_rules! try_get {
	($($call:ident).+($($args:expr),*)) => {
		unsafe {
			let result = $($call).*($($args),*);
			if result.is_null() {
				return ::std::result::Result::Err(::ui::windows::helpers::WinApiError(::winapi::um::errhandlingapi::GetLastError() as u64, ::failure::Backtrace::new()));
			}
			result
		}
	};
}

macro_rules! try_call {
	($($call:ident).+($($args:expr),*), $error_value:expr) => {
		unsafe {
			let result = $($call).*($($args),*);
			if result == $error_value {
				return ::std::result::Result::Err(::ui::windows::helpers::WinApiError(::winapi::um::errhandlingapi::GetLastError() as u64, ::failure::Backtrace::new()));
			}
			result
		}
	};
}

macro_rules! try_send_message {
	($($args:expr),*) => {
		unsafe {
			::winapi::um::winuser::SendMessageW($($args),*)
		}
	};
	($($args:expr),*; $($error_value:expr),*) => {
		unsafe {
			let result = ::winapi::um::winuser::SendMessageW($($args),*);
			$(
				if result == $error_value {
					return ::std::result::Result::Err(::ui::windows::helpers::WinApiError(result as u64, ::failure::Backtrace::new()));
				}
			)*
			result
		}
	};
}

pub type WideString = Vec<u16>;

pub fn to_wstring(string: &str) -> WideString {
	let mut data: Vec<u16> = OsStr::new(string).encode_wide().collect();
	data.push(0);
	data
}

pub fn from_wstring(string: PWSTR) -> String {
	let string_slice = unsafe {
		let len = wcslen(string);
		slice::from_raw_parts(string, len)
	};

	OsString::from_wide(string_slice).into_string().unwrap()
}

pub struct ComPtr<T: Interface> {
	ptr: *mut T
}

impl<T: Interface> ComPtr<T> {
	pub fn new() -> ComPtr<T> {
		ComPtr {
			ptr: null_mut()
		}
	}

	pub fn as_out_param<O>(&mut self) -> *mut *mut O {
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

pub struct ComMemPtr<T> {
	ptr: *mut T
}

impl<T> ComMemPtr<T> {
	pub fn new() -> ComMemPtr<T> {
		ComMemPtr {
			ptr: null_mut()
		}
	}

	pub fn as_out_param(&mut self) -> *mut *mut T {
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

#[derive(Fail, Debug)]
#[fail(display = "Win API call failed, error {}", _0)]
pub struct WinApiError(pub u64, pub Backtrace);

impl From<isize> for WinApiError {
	fn from(code: isize) -> WinApiError {
		WinApiError(code as u64, Backtrace::new())
	}
}

pub struct MenuHandle {
	handle: HMENU
}

impl MenuHandle {
	pub fn load(name: &str) -> Result<MenuHandle, WinApiError> {
		let handle = try_get!(LoadMenuW(0 as HINSTANCE, to_wstring(name).as_ptr()));
		Ok(MenuHandle {
			handle
		})
	}

	pub fn handle(&self) -> HMENU {
		self.handle
	}
}

impl Drop for MenuHandle {
	fn drop(&mut self) {
		unsafe {
			DestroyMenu(self.handle);
		}
	}
}