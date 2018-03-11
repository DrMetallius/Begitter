#[macro_use]
extern crate nom;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;

#[cfg(test)]
extern crate tempdir;

mod patch_editor;
mod git;
mod parsing_utils;
