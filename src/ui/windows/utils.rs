use std::mem;

use winapi::shared::minwindef::{DWORD, UINT, LPARAM, BOOL, TRUE, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{WM_SETFONT, SPI_GETNONCLIENTMETRICS, GetWindowRect, SendMessageW, EnumChildWindows};
use winapi::um::wingdi::CreateFontIndirectW;

use ui::windows::dpi::{GetDpiForWindow, NONCLIENTMETRICS, SystemParametersInfoForDpi};
use ui::windows::helpers::WinApiError;
use winapi::um::winuser::MapWindowPoints;
use std::ptr::null_mut;
use winapi::um::errhandlingapi::{SetLastError, GetLastError};

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
					return Err(WinApiError(error as u64));
				}
			}
		}
	}

	Ok(rect)
}