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

pub mod patch_editor;
pub mod git;
pub mod change_set;
mod parsing_utils;
