#![windows_subsystem = "windows"]

extern crate begitter;
#[macro_use]
extern crate failure;
#[cfg(windows)]
extern crate libc;
#[cfg(windows)]
extern crate winapi;
extern crate core;

mod ui;

fn main() {
	if cfg!(windows) {
		ui::windows::main::run().unwrap();
	}
}
