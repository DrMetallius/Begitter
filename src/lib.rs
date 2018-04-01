#[macro_use]
extern crate nom;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;

#[cfg(test)]
extern crate tempdir;
extern crate winapi;

pub mod patch_editor;
pub mod git;
mod parsing_utils;
