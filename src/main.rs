#![windows_subsystem = "windows"]
#![feature(trace_macros)]

extern crate begitter;

#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate libc;

mod ui;

fn main() {
	ui::windows::run().unwrap();
}
