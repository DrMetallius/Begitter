#![windows_subsystem = "windows"]

extern crate begitter;

#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate libc;

mod ui;

fn main() {
	ui::windows::main::run().unwrap();
}
