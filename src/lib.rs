#[macro_use]
extern crate nom;
#[macro_use]
extern crate failure;

extern crate winapi;
extern crate time;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;

#[cfg(test)]
extern crate tempdir;
extern crate pathdiff;

pub mod patch_editor;
pub mod git;
pub mod change_set;
pub mod model;
mod parsing_utils;
