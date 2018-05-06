use winapi::shared::minwindef::UINT;
use ui::windows::helpers::WinApiError;
use winapi::um::libloaderapi::LoadStringW;
use std::ptr::null_mut;
use std::slice::from_raw_parts;

pub const STRING_MAIN_WINDOW_NAME: UINT = 1;
pub const STRING_MAIN_BRANCHES: UINT = 2;
pub const STRING_MAIN_COMMITS: UINT = 3;
pub const STRING_MAIN_PATCHES: UINT = 4;

pub fn load_string(id: UINT) -> Result<Vec<u16>, WinApiError> {
	let mut string_pointer = null_mut::<u16>();
	let string_length = try_call!(LoadStringW(null_mut(), id, &mut string_pointer as *mut _ as *mut u16, 0), 0);
	let string_slice = unsafe { from_raw_parts(string_pointer, string_length as usize) };

	let mut string = Vec::with_capacity(string_length as usize);
	string.extend(string_slice);
	string.push(0u16);

	Ok(string)
}