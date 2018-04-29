
use winapi::shared::minwindef::{UINT, BOOL};
use winapi::shared::ntdef::PVOID;
use winapi::shared::windef::HWND;
use winapi::ctypes::c_int;
use winapi::um::wingdi::LOGFONTW;

#[allow(non_snake_case)]
#[repr(C)]
pub struct NONCLIENTMETRICS {
	pub cbSize: UINT,
	pub iBorderWidth: c_int,
	pub iScrollWidth: c_int,
	pub iScrollHeight: c_int,
	pub iCaptionWidth: c_int,
	pub iCaptionHeight: c_int,
	pub lfCaptionFont: LOGFONTW,
	pub iSmCaptionWidth: c_int,
	pub iSmCaptionHeight: c_int,
	pub lfSmCaptionFont: LOGFONTW,
	pub iMenuWidth: c_int,
	pub iMenuHeight: c_int,
	pub lfMenuFont: LOGFONTW,
	pub lfStatusFont: LOGFONTW,
	pub lfMessageFont: LOGFONTW,
	pub iPaddedBorderWidth: c_int
}

#[allow(non_snake_case)]
extern "system" {
	pub fn GetDpiForWindow(hwnd: HWND) -> UINT;
	pub fn SystemParametersInfoForDpi(uiAction: UINT, uiParam: UINT, pvParam: PVOID, fWinIni: UINT, dpi: UINT) -> BOOL;
}