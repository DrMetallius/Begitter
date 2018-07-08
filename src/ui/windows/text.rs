use std::ptr::null_mut;
use std::slice::from_raw_parts;
use std::ops::Range;

use time;
use winapi::shared::minwindef::UINT;
use winapi::um::libloaderapi::LoadStringW;

use ui::windows::helpers::WinApiError;

pub const STRING_MAIN_WINDOW_NAME: UINT = 1;
pub const STRING_MAIN_BRANCHES: UINT = 2;
pub const STRING_MAIN_COMMITS: UINT = 3;
pub const STRING_MAIN_PATCHES: UINT = 4;
pub const STRING_MAIN_RESOLVE_REJECTS: UINT = 5;
pub const STRING_MAIN_RESOLVE_CONFLICTS: UINT = 6;
pub const STRING_MAIN_ABORT: UINT = 7;

pub const STRING_MAIN_COMMITS_COLUMN_MESSAGE: UINT = 8;
pub const STRING_MAIN_COMMITS_COLUMN_HASH: UINT = 11;
pub const STRING_MAIN_COMMITS_COLUMNS: Range<UINT> = STRING_MAIN_COMMITS_COLUMN_MESSAGE..STRING_MAIN_COMMITS_COLUMN_HASH + 1;

pub const STRING_MAIN_PATCHES_COLUMN_MESSAGE: UINT = 12;
pub const STRING_MAIN_PATCHES_COLUMN_DATE: UINT = 14;
pub const STRING_MAIN_PATCHES_COLUMNS: Range<UINT> = STRING_MAIN_PATCHES_COLUMN_MESSAGE..STRING_MAIN_PATCHES_COLUMN_DATE + 1;

pub const STRING_REJECTS_ACCEPT_HUNK: UINT = 15;
pub const STRING_REJECTS_UNACCEPT_HUNK: UINT = 16;

pub fn load_string(id: UINT) -> Result<Vec<u16>, WinApiError> {
	let mut string_pointer = null_mut::<u16>();
	let string_length = try_call!(LoadStringW(null_mut(), id, &mut string_pointer as *mut _ as *mut u16, 0), 0);
	let string_slice = unsafe { from_raw_parts(string_pointer, string_length as usize) };

	let mut string = Vec::with_capacity(string_length as usize);
	string.extend(string_slice);
	string.push(0u16);

	Ok(string)
}

pub fn format_time(time_spec: time::Timespec) -> String {
	time::strftime("%Y-%m-%d %H:%M:%S", &time::at(time_spec)).unwrap()
}