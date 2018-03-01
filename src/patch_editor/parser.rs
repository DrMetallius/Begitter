use nom::{anychar, digit, is_space, is_hex_digit, is_oct_digit, line_ending, not_line_ending, space, Needed};
use nom::{IResult, IError};
use nom::IResult::{Done, Incomplete};
use std::string::FromUtf8Error;
use std::num::ParseIntError;
use std::fmt::Debug;
use super::patch::{Patch, FileProperties, Hunk, Operation};
	Removed,
		mode: Option<&'a [u8]>,
	Hunk(Hunk<'a>),
struct PatchParts<'a> {
	names: Option<(Vec<u8>, Vec<u8>)>,
	parts: Vec<PatchPart<'a>>,
}

#[derive(Debug)]
pub enum ParseError {
	LexerError(IError<u32>),
	EncodingError(FromUtf8Error),
	IntValueError(ParseIntError),
	PartConflict(String),
	PartAbsent(&'static str)
}

impl From<IError<u32>> for ParseError {
	fn from(error: IError<u32>) -> Self {
		ParseError::LexerError(error)
	}
}

impl From<FromUtf8Error> for ParseError {
	fn from(error: FromUtf8Error) -> Self {
		ParseError::EncodingError(error)
	}
}

impl From<ParseIntError> for ParseError {
	fn from(error: ParseIntError) -> Self {
		ParseError::IntValueError(error)
	}
}

fn check_and_update_string_value(value: Vec<u8>, order: Order, old_value_ref: &mut Option<String>, new_value_ref: &mut Option<String>) -> Result<(), ParseError> {
	let value_str = String::from_utf8(value)?;
	let value_ref = match order {
		Order::Old => old_value_ref,
		Order::New => new_value_ref
	};
	update_if_absent(value_ref, value_str)
}

fn update_if_absent<T: PartialEq + Debug>(value_ref: &mut Option<T>, value: T) -> Result<(), ParseError> {
	if value_ref.is_none() {
		*value_ref = Some(value);
	} else {
		let curr_value = value_ref.as_ref().unwrap();
		if *curr_value != value {
			return Err(ParseError::PartConflict(format!("New value {:?} conflicts with {:?}", value, curr_value)));
		}
	}
	Ok(())
}

struct Parser<'a> {
	old_name: Option<String>,
	new_name: Option<String>,
	operation: Option<Operation>,
	old_mode: Option<String>,
	new_mode: Option<String>,
	old_index: Option<String>,
	new_index: Option<String>,
	similarity: Option<u8>,
	hunks: Vec<Hunk<'a>>,
}

// To get the patch, run "git log --follow -p -1 --format= <file-path>"
pub fn parse_patch<'a>(input: &'a [u8]) -> Result<Patch<'a>, ParseError> {
	let PatchParts { names, parts } = read_patch(input).to_full_result()?;

	let mut parser = Parser {
		old_name: None,
		new_name: None,
		operation: None,
		old_mode: None,
		new_mode: None,
		old_index: None,
		new_index: None,
		similarity: None,
		hunks: Vec::new(),
	};

	if let Some((header_old_name, header_new_name)) = names {
		parser.old_name = Some(String::from_utf8(header_old_name)?);
		parser.new_name = Some(String::from_utf8(header_new_name)?);
	}

	for part in parts {
		match part {
			PatchPart::Name(name, order) => check_and_update_string_value(name, order, &mut parser.old_name, &mut parser.new_name)?,
			PatchPart::NameChange(name, change_type, order) => {
				check_and_update_string_value(name, order, &mut parser.old_name, &mut parser.new_name)?;

				let parsed_operation = match change_type {
					NameChangeType::Rename => Operation::Renamed,
					NameChangeType::Copy => Operation::Copied,
				};
				update_if_absent(&mut parser.operation, parsed_operation)?;
			}
			PatchPart::PresenceChange { change_type, mode } => {
				let (parsed_operation, mode_order) = match change_type {
					PresenceChangeType::Added => (Operation::Added, Order::New),
					PresenceChangeType::Removed => (Operation::Removed, Order::Old),
				};
				update_if_absent(&mut parser.operation, parsed_operation)?;
				check_and_update_string_value(mode.to_vec(), mode_order, &mut parser.old_mode, &mut parser.new_mode)?;
			}
			PatchPart::ModeChange(mode, order) => {
				check_and_update_string_value(mode.to_vec(), order, &mut parser.old_mode, &mut parser.new_mode)?;
			}
			PatchPart::Index { old_index, new_index, mode } => {
				let old_index_str = String::from_utf8(old_index.to_vec())?;
				update_if_absent(&mut parser.old_index, old_index_str);

				let new_index_str = String::from_utf8(new_index.to_vec())?;
				update_if_absent(&mut parser.new_index, new_index_str);

				if let Some(mode_data) = mode {
					let mode_str = String::from_utf8(mode_data.to_vec())?;
					update_if_absent(&mut parser.old_mode, mode_str.clone());
					update_if_absent(&mut parser.new_mode, mode_str);
				}
			}
			PatchPart::Similarity(similarity) => {
				update_if_absent(&mut parser.similarity, String::from_utf8(similarity.to_vec())?.parse()?);
			}
			PatchPart::Dissimilarity(dissimilarity) => {
				update_if_absent(&mut parser.similarity, 100 - String::from_utf8(dissimilarity.to_vec())?.parse::<u8>()?);
			}
			PatchPart::Hunk(hunk) => parser.hunks.push(hunk)
		}
	}

	Ok(Patch {
		operation: parser.operation.unwrap_or(Operation::Edited),
		old_properties: FileProperties {
			name: parser.old_name.ok_or(ParseError::PartAbsent("Old name"))?,
			mode: parser.old_mode.ok_or(ParseError::PartAbsent("Old mode"))?,
			index: parser.old_index.ok_or(ParseError::PartAbsent("Old index"))?,
		},
		new_properties: FileProperties {
			name: parser.new_name.ok_or(ParseError::PartAbsent("New name"))?,
			mode: parser.new_mode.ok_or(ParseError::PartAbsent("New mode"))?,
			index: parser.new_index.ok_or(ParseError::PartAbsent("New index"))?,
		},
		hunks: parser.hunks,
	})
}

fn read_patch(input: &[u8]) -> IResult<&[u8], PatchParts> {
	let (unparsed, names) = try_parse!(input, parse_header);
	let mut rest = unparsed;

	let mut parts: Vec<PatchPart> = Vec::new();
	while !rest.is_empty() {
		let (unparsed, part) = try_parse!(rest, patch_part);
		rest = unparsed;
		parts.push(part);
	}

	Done(rest, PatchParts {
		names,
		parts,
	})
	parse_header<Option<(Vec<u8>, Vec<u8>)>>,
				(Some((name, other_name)))
				(Some((name, other_name)))
fn matching_name_pair(input: &[u8]) -> IResult<&[u8], Option<(Vec<u8>, Vec<u8>)>> {
					return Done(&input[line_end + 1..], Some((name, other_name)));
	return Done(&input[line_end..], None);
		mode: opt!(preceded!(tag!(b" "), take_while!(is_oct_digit))) >>
			mode: mode
		change_type: alt!(value!(PresenceChangeType::Removed, tag!("deleted file mode ")) | value!(PresenceChangeType::Added, tag!("new file mode "))) >>
		name: map_opt!(file_name, trim_to_slash_inclusive) >>
		data: &input[..bytes_consumed_total],
		let (name, other_name) = parse_header(header).to_result().unwrap().unwrap();
			data: &hunk_data[hunk_data.iter().position(|byte| *byte == b'\n').unwrap() + 1..],
		let result = parse_patch(patch_data).unwrap();
		assert_eq!(result, Patch {
			operation: Operation::Edited,
			old_properties: FileProperties {
				name: "gradle.properties".into(),
				mode: "100644".into(),
				index: "aac7c9b".into(),
			},
			new_properties: FileProperties {
				name: "gradle.properties".into(),
				mode: "100644".into(),
				index: "f33a6d7".into(),
			},
			hunks: vec![Hunk {
				old_file_range: 1..10,
				new_file_range: 1..4,
				data: br#"-# Project-wide Gradle settings.
-
-# IDE (e.g. Android Studio) users:
-# Gradle settings configured through the IDE *will override*
-# any settings specified in this file.
-
 # For more details on how to configure your build environment visit
 # http://www.gradle.org/docs/current/userguide/build_environment.html

"#,
			}, Hunk {
				old_file_range: 14..18,
				new_file_range: 8..12,
				data: br#" # When configured, Gradle will run in incubating parallel mode.
 # This option should only be used with decoupled projects. More details, visit
 # http://www.gradle.org/docs/current/userguide/multi_project_builds.html#sec:decoupled_projects
-# org.gradle.parallel=true
+org.gradle.parallel=true
"#,
			}],
		});