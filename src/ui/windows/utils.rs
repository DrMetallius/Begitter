use std::mem;

use winapi::shared::minwindef::{UINT, LPARAM, BOOL, TRUE, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{WM_SETFONT, SPI_GETNONCLIENTMETRICS, SendMessageW, EnumChildWindows};
use winapi::um::wingdi::CreateFontIndirectW;

use ui::windows::dpi::{GetDpiForWindow, NONCLIENTMETRICS, SystemParametersInfoForDpi};
use ui::windows::helpers::WinApiError;

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