use nom::{anychar, digit, is_space, is_hex_digit, is_oct_digit, line_ending, not_line_ending, space, Needed, IResult, ErrorKind, Err, newline};
use std::ops::Range;
use std::borrow::Cow;
use std::string::FromUtf8Error;
use std::num::ParseIntError;
use std::fmt::Debug;

use super::patch::{Change, Patch, FileProperties, Hunk, ModificationType};
use super::super::parsing_utils::{file_name, quoted_name};

#[derive(Debug, Eq, PartialEq)]
enum Order {
	Old,
	New,
}

#[derive(Debug, Eq, PartialEq)]
enum NameChangeType {
	Rename,
	Copy,
}

#[derive(Debug, Eq, PartialEq)]
enum PresenceChangeType {
	Added,
	Removed,
}

#[derive(Debug, Eq, PartialEq)]
enum PatchPart<'a> {
	Name(Vec<u8>, Order),
	NameChange(Vec<u8>, NameChangeType, Order),
	PresenceChange {
		change_type: PresenceChangeType,
		mode: &'a [u8],
	},
	ModeChange(&'a [u8], Order),
	Similarity(&'a [u8]),
	Dissimilarity(&'a [u8]),
	Index {
		old_index: &'a [u8],
		new_index: &'a [u8],
		mode: Option<&'a [u8]>,
	},
	Hunk(Hunk),
}

struct PatchParts<'a> {
	names: Option<(Vec<u8>, Vec<u8>)>,
	parts: Vec<PatchPart<'a>>,
}

#[derive(Fail, Debug)]
pub enum ParseError {
	#[fail(display = "Unable to read the patch: {:?}", _0)]
	LexerError(ErrorKind<u32>),
	#[fail(display = "Error when parsing Unicode input: {}", _0)]
	EncodingError(FromUtf8Error),
	#[fail(display = "Error when parsing an int value: {}", _0)]
	IntValueError(ParseIntError),
	#[fail(display = "{}", _0)]
	PartConflict(String),
	#[fail(display = "Absent part error: {}", _0)]
	PartAbsent(&'static str),
}

impl<I> From<Err<I, u32>> for ParseError {
	fn from(error: Err<I, u32>) -> Self {
		ParseError::LexerError(error.into_error_kind())
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

struct Parser {
	old_name: Option<String>,
	new_name: Option<String>,
	operation: Option<Operation>,
	old_mode: Option<String>,
	new_mode: Option<String>,
	old_index: Option<String>,
	new_index: Option<String>,
	similarity: Option<u8>,
	hunks: Vec<Hunk>,
}

impl Parser {
	fn old_properties(&self) -> Result<FileProperties, ParseError> {
		Ok(FileProperties {
			name: self.old_name.clone().ok_or(ParseError::PartAbsent("Old name"))?,
			mode: self.old_mode.clone().ok_or(ParseError::PartAbsent("Old mode"))?,
			index: self.old_index.clone().ok_or(ParseError::PartAbsent("Old index"))?,
		})
	}

	fn new_properties(&self) -> Result<FileProperties, ParseError> {
		Ok(FileProperties {
			name: self.new_name.clone().ok_or(ParseError::PartAbsent("New name"))?,
			mode: self.new_mode.clone().ok_or(ParseError::PartAbsent("New mode"))?,
			index: self.new_index.clone().ok_or(ParseError::PartAbsent("New index"))?,
		})
	}
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum Operation {
	Added,
	Removed,
	Copied,
	Renamed,
	ModeChanged,
	Edited,
}

pub fn parse_combined_patch<'a>(input: &'a [u8]) -> Result<Vec<Patch>, ParseError> {
	let (_, patch_parts_vec) = combined_patch(input)?;
	patch_parts_vec.into_iter()
			.map(|patch_parts| patch_from_parts(patch_parts))
			.collect()
}

// To get the patch, run "git log --follow -p -1 --format= <file-path>"
pub fn parse_patch(input: &[u8]) -> Result<Patch, ParseError> {
	let (_, patch_parts) = patch(input)?;
	patch_from_parts(patch_parts)
}

pub fn parse_rejects(input: &[u8]) -> Result<Vec<Hunk>, ParseError> {
	let result = do_parse!(input,
		not_line_ending >>
		line_ending >>
		hunks: many1!(complete!(hunk)) >>
		(hunks)
	);

	match result {
		Ok((_, hunks)) => Ok(hunks),
		Err(err) => Err(err.into())
	}
}

fn patch_from_parts<'a>(PatchParts { names, parts }: PatchParts<'a>) -> Result<Patch, ParseError> {
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
				update_if_absent(&mut parser.old_index, old_index_str)?;

				let new_index_str = String::from_utf8(new_index.to_vec())?;
				update_if_absent(&mut parser.new_index, new_index_str)?;

				if let Some(mode_data) = mode {
					let mode_str = String::from_utf8(mode_data.to_vec())?;
					update_if_absent(&mut parser.old_mode, mode_str.clone())?;
					update_if_absent(&mut parser.new_mode, mode_str)?;
				}
			}
			PatchPart::Similarity(similarity) => {
				update_if_absent(&mut parser.similarity, String::from_utf8(similarity.to_vec())?.parse()?)?;
			}
			PatchPart::Dissimilarity(dissimilarity) => {
				update_if_absent(&mut parser.similarity, 100 - String::from_utf8(dissimilarity.to_vec())?.parse::<u8>()?)?;
			}
			PatchPart::Hunk(hunk) => parser.hunks.push(hunk)
		}
	}

	let operation = parser.operation.clone().unwrap_or(Operation::Edited);

	let change = match operation {
		Operation::Added => Change::Addition { new_properties: parser.new_properties()? },
		Operation::Removed => Change::Removal { old_properties: parser.old_properties()? },
		other_operation => {
			Change::Modification {
				modification_type: match other_operation {
					Operation::ModeChanged => ModificationType::ModeChanged,
					Operation::Renamed => ModificationType::Renamed { similarity: parser.similarity },
					Operation::Copied => ModificationType::Copied { similarity: parser.similarity },
					Operation::Edited => ModificationType::Edited,
					_ => panic!("Unhandled operation {:?}", operation)
				},
				old_properties: parser.old_properties()?,
				new_properties: parser.new_properties()?,
			}
		}
	};

	Ok(Patch {
		change,
		hunks: parser.hunks,
	})
}

named!(combined_patch<Vec<PatchParts>>, many1!(complete!(patch)));

named!(
	patch<PatchParts>,
	do_parse!(
		names: patch_header >>
		parts: many1!(complete!(patch_part)) >>
		(PatchParts {
			names,
			parts
		})
	)
);

named!(
	patch_header<Option<(Vec<u8>, Vec<u8>)>>,
	do_parse!(
		tag!(b"diff --git ") >>
		names: alt!(
			do_parse!(
				name: map_opt!(quoted_name, trim_to_slash_inclusive) >>
				space >>
				other_name: map_opt!(file_name, trim_to_slash_inclusive) >>
				line_ending >>
				(Some((name, other_name)))
			) |
			do_parse!(
				name: map_opt!(map!(take_until_either!("\"\n"), |name| trim_right(name)), trim_to_slash_inclusive) >>
				other_name: map_opt!(quoted_name, trim_to_slash_inclusive) >>
				line_ending >>
				(Some((name, other_name)))
			) |
			matching_name_pair
		) >>
		(names)
	)
);

fn trim_to_slash_inclusive<'a, I: Into<Cow<'a, [u8]>>>(input: I) -> Option<Vec<u8>> {
	let slice = input.into();
	match slice.iter().position(|byte| *byte == b'/') {
		Some(index) => Some(slice.into_owned().split_off(index + 1)),
		None => None
	}
}

fn trim_right(input: &[u8]) -> &[u8] {
	match input.iter().rposition(|byte| !is_space(*byte)) {
		Some(position) => &input[..position + 1],
		None => input
	}
}

fn matching_name_pair(input: &[u8]) -> IResult<&[u8], Option<(Vec<u8>, Vec<u8>)>> {
	let line_end = match input.iter().position(|item| *item == b'\n') {
		Some(position) => position,
		None => return Err(Err::Incomplete(Needed::Unknown))
	};

	let mut separator_start = 0;
	while separator_start < line_end {
		if !is_space(input[separator_start]) {
			separator_start += 1;
			continue;
		}

		let mut separator_end = separator_start;
		while separator_end < line_end && is_space(input[separator_end]) {
			separator_end += 1;
		}

		if let Some(name) = trim_to_slash_inclusive(&input[0..separator_start]) {
			if let Some(other_name) = trim_to_slash_inclusive(&input[separator_end..line_end]) {
				if name == other_name {
					return Ok((&input[line_end + 1..], Some((name, other_name))));
				}
			}
		}

		separator_start = separator_end + 1;
	}

	return Ok((&input[line_end..], None));
}

named!(
	patch_part<PatchPart>,
	alt_complete!(
		map!(hunk, |hunk| PatchPart::Hunk(hunk)) |
		similarity |
		name |
		name_change |
		mode_change |
		presence_change |
		index
	)
);

named!(
	index<PatchPart>,
	do_parse!(
		tag!("index ") >>
		hashes: separated_pair!(
			take_while_s!(is_hex_digit),
			tag!(".."),
			take_while_s!(is_hex_digit)
		) >>
		mode: opt!(preceded!(tag!(b" "), take_while!(is_oct_digit))) >>
		tag!("\n") >>
		(PatchPart::Index {
			old_index: hashes.0,
			new_index: hashes.1,
			mode: mode
		})
	)
);

named!(
	mode_change<PatchPart>,
	do_parse!(
		order: alt!(value!(Order::Old, tag!("old mode ")) | value!(Order::New, tag!("new mode "))) >>
		mode: take_while!(is_oct_digit) >>
		tag!("\n") >>
		(PatchPart::ModeChange(mode, order))
	)
);

named!(
	name_change<PatchPart>,
	do_parse!(
		change_type_and_order: alt!(
			value!((NameChangeType::Rename, Order::Old), alt!(tag!("rename old ") | tag!("rename from "))) |
			value!((NameChangeType::Rename, Order::New), alt!(tag!("rename new ") | tag!("rename to "))) |
			value!((NameChangeType::Copy, Order::Old), tag!("copy from ")) |
			value!((NameChangeType::Copy, Order::New), tag!("copy to "))
		) >>
		name: file_name >>
		tag!("\n") >>
		(PatchPart::NameChange(name, change_type_and_order.0, change_type_and_order.1))
	)
);

named!(
	presence_change<PatchPart>,
	do_parse!(
		change_type: alt!(value!(PresenceChangeType::Removed, tag!("deleted file mode ")) | value!(PresenceChangeType::Added, tag!("new file mode "))) >>
		mode: take_while!(is_oct_digit) >>
		tag!("\n") >>
		(PatchPart::PresenceChange {
			change_type: change_type,
			mode: mode
		})
	)
);

named!(
	name<PatchPart>,
	do_parse!(
		order: alt!(value!(Order::Old, tag!("--- ")) | value!(Order::New, tag!("+++ "))) >>
		name: map_opt!(file_name, trim_to_slash_inclusive) >>
		tag!("\n") >>
		(PatchPart::Name(name, order))
	)
);

named!(
	similarity<PatchPart>,
	do_parse!(
		dissimilarity_flag: alt!(value!(false, tag!("similarity index ")) | value!(true, tag!("dissimilarity index "))) >>
		score: digit >>
		tag!("\n") >>
		(if dissimilarity_flag { PatchPart::Similarity(score) } else { PatchPart::Dissimilarity(score) })
	)
);

named!(
	hunk<Hunk>,
	do_parse!(
		file_ranges: hunk_header >>
		hunk: apply!(hunk_data, file_ranges.0, file_ranges.1) >>
		(hunk)
	)
);

named!(
	hunk_header<(Range<usize>, Range<usize>)>,
	do_parse!(
		tag!("@@ -") >>
		old_file_range: range >>
		tag!(" +") >>
		new_file_range: range >>
		tag!(" @@") >>
		take_until!("\n") >>
		line_ending >>
		((old_file_range, new_file_range))
	)
);

named!(
	range<Range<usize>>,
	do_parse!(
		offset: digits_usize >>
		lines: opt!(preceded!(tag!(","), digits_usize)) >>
		(offset..offset + lines.unwrap_or(1usize))
	)
);

named!(
	digits_usize<usize>,
	map_opt!(digit, |digits: &[u8]| {
		String::from_utf8(digits.into())
			.ok()
			.and_then(|number| number.parse().ok())
	})
);

fn hunk_data(input: &[u8], old_file_range: Range<usize>, new_file_range: Range<usize>) -> IResult<&[u8], Hunk> {
	let mut old_file_lines_left = old_file_range.end - old_file_range.start;
	let mut new_file_lines_left = new_file_range.end - new_file_range.start;

	let mut rest = input;
	let mut bytes_consumed_total = 0;
	while old_file_lines_left > 0 || new_file_lines_left > 0 {
		let (new_rest, ((old_file_lines_consumed, new_file_lines_consumed), bytes_consumed)) = try_parse!(rest, hunk_line);

		rest = new_rest;
		old_file_lines_left -= old_file_lines_consumed;
		new_file_lines_left -= new_file_lines_consumed;
		bytes_consumed_total += bytes_consumed;

		if old_file_lines_left == 0 && old_file_lines_consumed > 0 || new_file_lines_left == 0 && new_file_lines_consumed > 0 {
			let (new_rest, newline_absence_indicator_length) = try_parse!(rest, no_new_line_marker);
			if let Some(bytes_consumed) = newline_absence_indicator_length {
				bytes_consumed_total += bytes_consumed;
			}
			rest = new_rest;
		}
	}

	Ok((&input[bytes_consumed_total..], Hunk {
		old_file_range,
		new_file_range,
		data: input[..bytes_consumed_total].into(),
	}))
}

named!(
	hunk_line<((usize, usize), usize)>,
	do_parse!(
		lines_consumed_in_both_files: switch!(peek!(anychar),
			'\n' => value!((1, 1)) |
			' ' => value!((1, 1)) |
			'-' => value!((1, 0)) |
			'+' => value!((0, 1))
		) >>
		line: take_until!("\n") >>
		line_ending >>
		((lines_consumed_in_both_files, line.len() + 1))
	)
);

named!(
	no_new_line_marker<Option<usize>>,
	opt!(
		complete!(
			do_parse!(
				tag!("\\") >>
				line: take_until!("\n") >>
				line_ending >>
				(line.len() + 2)
			)
		)
	)
);

#[cfg(test)]
mod test {
	use super::*;
	use super::super::test_data::*;

	fn match_name(header: &[u8], expected_name: &[u8]) {
		let (name, other_name) = patch_header(header).unwrap().1.unwrap();
		assert_eq!(name, other_name);
		assert_eq!(name.as_slice(), expected_name);
	}

	#[test]
	fn test_patch_header() {
		match_name(b"diff --git \"a/gradle.properties\" \"b/gradle.properties\"\n", b"gradle.properties");
		match_name(b"diff --git \"a/gradle.properties\" b/gradle.properties\n", b"gradle.properties");
		match_name(b"diff --git a/gradle.properties \"b/gradle.properties\"\n", b"gradle.properties");
		match_name(b"diff --git a/gradle.properties b/gradle.properties\n", b"gradle.properties");
	}

	#[test]
	fn test_unquote() {
		assert_eq!(quoted_name(br#""Test""#), Ok((&b""[..], (&b"Test"[..]).into())));
		assert_eq!(quoted_name(br#""Te\\s\"t\n""#), Ok((&b""[..], (&b"Te\\s\"t\n"[..]).into())));
		assert_eq!(quoted_name(br#""Test\040""#), Ok((&b""[..], (&b"Test "[..]).into())));
	}

	#[test]
	fn test_hunk_header() {
		let header = b"@@ -14,4 +8,4 @@ org.gradle.jvmargs=-Xmx1536m\n";
		assert_eq!(hunk_header(header), Ok((&b""[..], (14..18, 8..12))));
	}

	#[test]
	fn test_range() {
		assert_eq!(range(b"14,5 "), Ok((&b" "[..], 14..19)));
		assert_eq!(range(b"14 "), Ok((&b" "[..], 14..15)));
	}

	fn match_line(line: &[u8], old_file_lines_consumed: usize, new_file_lines_consumed: usize) {
		assert_eq!(hunk_line(line), Ok((&b""[..], ((old_file_lines_consumed, new_file_lines_consumed), line.len()))));
	}

	#[test]
	fn test_hunk_line() {
		match_line(b" # When configured, Gradle will run in incubating parallel mode.\n", 1, 1);
		match_line(b"-# org.gradle.parallel=true\n", 1, 0);
		match_line(b"+org.gradle.parallel=true\n", 0, 1);
	}

	#[test]
	fn test_digits_usize() {
		assert_eq!(digits_usize(b"14data"), Ok((&b"data"[..], 14)));
	}

	#[test]
	fn test_hunk() {
		assert_eq!(hunk(&**PATCH_DATA_HUNK_2), Ok((&b""[..], generate_hunk_2())));
	}

	#[test]
	fn test_parse_patch() {
		let result = parse_patch(&*PATCH_DATA).unwrap();
		assert_eq!(result, *PATCH);
	}

	#[test]
	fn test_parse_patch_quoted() {
		let result = parse_patch(PATCH_ADDITION_DATA).unwrap();
		assert_eq!(result, *PATCH_ADDITION);
	}

	#[test]
	fn test_parse_patch_overlapping_hunks() {
		let result = parse_patch(&*PATCH_DATA_OVERLAPPING_HUNKS);
		assert_eq!(result.is_err(), true);
	}

	#[test]
	fn test_parse_patch_no_new_lines() {
		let result = parse_patch(&*PATCH_DATA_NO_NEW_LINES).unwrap();
		assert_eq!(result, *PATCH_NO_NEW_LINES);
	}

	#[test]
	fn test_parse_combined_patch() {
		let result = parse_combined_patch(&*COMBINED_PATCH_DATA).unwrap();
		assert_eq!(result.iter().collect::<Vec<&Patch>>(), *COMBINED_PATCH);
	}

	#[test]
	fn test_parse_rejects() {
		let result = parse_rejects(&*REJECTS_DATA).unwrap();
		assert_eq!(result, *REJECTED_HUNKS);
	}
}
