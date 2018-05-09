use std::mem;
use std::ptr::null_mut;
use std::ops::Range;

use winapi::shared::minwindef::{DWORD, UINT, LPARAM, BOOL, TRUE, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::commctrl::{LVITEMW, LVCOLUMNW, LVM_INSERTITEMW, LVM_INSERTCOLUMNW, LVCF_TEXT, LVCF_SUBITEM, LVCF_WIDTH, LVCF_FMT, LVCFMT_LEFT,
	LVIF_TEXT, LVIF_STATE, LPSTR_TEXTCALLBACKW};
use winapi::um::winuser::{WM_SETFONT, SPI_GETNONCLIENTMETRICS, GetWindowRect, SendMessageW, EnumChildWindows, MapWindowPoints};
use winapi::um::wingdi::CreateFontIndirectW;
use winapi::um::errhandlingapi::{SetLastError, GetLastError};
use winapi::ctypes::c_int;
use failure::Backtrace;

use ui::windows::dpi::{GetDpiForWindow, NONCLIENTMETRICS, SystemParametersInfoForDpi};
use ui::windows::helpers::WinApiError;
use ui::windows::text::load_string;

pub fn set_fonts(main_window: HWND) -> Result<(), WinApiError> {
	let dpi = try_call!(GetDpiForWindow(main_window), 0);

	let mut non_client_metrics: NONCLIENTMETRICS = unsafe { mem::uninitialized() };
	let non_client_metrics_size = mem::size_of_val(&non_client_metrics) as UINT;
	non_client_metrics.cbSize = non_client_metrics_size;

	try_call!(SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, non_client_metrics_size, &mut non_client_metrics as *mut _ as *mut _, 0, dpi), 0);
	let message_font = try_get!(CreateFontIndirectW(&non_client_metrics.lfMessageFont));

	extern "system" fn set_font(child: HWND, font: LPARAM) -> BOOL {
		unsafe {
			SendMessageW(child, WM_SETFONT, font as WPARAM, TRUE as LPARAM);
		}
		TRUE
	}

	unsafe {
		EnumChildWindows(main_window, Some(set_font), message_font as LPARAM);
	}
	Ok(())
}

pub fn get_window_position(window: HWND, reference_window: HWND) -> Result<RECT, WinApiError> {
	let mut rect = RECT {
		top: 0,
		left: 0,
		right: 0,
		bottom: 0
	};
	try_call!(GetWindowRect(window, &mut rect as *mut _ as *mut _), 0);

	if window != reference_window {
		unsafe {
			SetLastError(0 as DWORD);
			let result = MapWindowPoints(null_mut(), reference_window,&mut rect as *mut _ as *mut _, 2);
			if result == 0 {
				let error = GetLastError();
				if error != 0 {
					return Err(WinApiError(error as u64, Backtrace::new()));
				}
			}
		}
	}

	Ok(rect)
}

pub fn insert_columns_into_list_view(list_view: HWND, text_range: Range<UINT>) -> Result<(), WinApiError> {
	let mut column: LVCOLUMNW = unsafe { mem::zeroed() };
	column.mask = LVCF_TEXT | LVCF_SUBITEM | LVCF_WIDTH | LVCF_FMT;

	for (index, text_id) in text_range.enumerate() {
		let mut name = load_string(text_id)?;
		column.fmt = LVCFMT_LEFT;
		column.iSubItem = index as c_int;
		column.pszText = name.as_mut_ptr();
		column.cx = 200;
		try_send_message!(list_view, LVM_INSERTCOLUMNW, index, &mut column as *mut _ as LPARAM; -1);
	}

	Ok(())
}

pub fn insert_rows_into_list_view(list_view: HWND, column_count: usize) -> Result<(), WinApiError> {
	let mut item: LVITEMW = unsafe { mem::zeroed() };
	item.mask = LVIF_TEXT | LVIF_STATE;
	item.pszText = LPSTR_TEXTCALLBACKW;
	item.iSubItem = 0;
	item.state = 0;
	item.stateMask = 0;

	for index in 0..column_count {
		item.iItem = index as c_int;
		try_send_message!(list_view, LVM_INSERTITEMW, 0, &item as *const _ as LPARAM; -1);
	}

	Ok(())
}
