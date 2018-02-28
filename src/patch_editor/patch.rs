use std::ops::Range;

#[derive(Debug, Eq, PartialEq)]
pub struct Patch<'a> {
	pub operation: Operation,
	pub old_properties: FileProperties,
	pub new_properties: FileProperties,
	pub hunks: Vec<Hunk<'a>>
}

#[derive(Debug, Eq, PartialEq)]
pub struct FileProperties {
	pub name: String,
	pub mode: String,
	pub index: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Operation {
	Added,
	Removed,
	Copied,
	Renamed,
	ModeChanged,
	Edited,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Hunk<'a> {
	pub old_file_range: Range<usize>,
	pub new_file_range: Range<usize>,
	pub data: &'a [u8],
}