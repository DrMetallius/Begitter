use nom::not_line_ending;
use nom::space;
use nom::{anychar, digit, is_space, is_hex_digit, is_oct_digit, line_ending, ErrorKind, Needed};
use nom::IResult;
use nom::IResult::{Done, Error, Incomplete};
use std::ops::Range;
use std::borrow::Cow;

// To get the patch, run "git log --follow -p -1 --format= <file-path>"
pub fn _parse_patch(input: &[u8]) -> IResult<&[u8], ()> {
	let (rest, (name, other_name)) = try_parse!(input, parse_header);
	let patch_parts = try_parse!(rest, patch_part);
	Done(rest, ())
}

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
	Deleted,
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
		old_mode: Option<&'a [u8]>,
	},
	Hunk(Hunk<'a>)
}

#[derive(Debug, Eq, PartialEq)]
struct Hunk<'a> {
	old_file_range: Range<usize>,
	new_file_range: Range<usize>,
	data: &'a [u8]
}

named!(
    parse_header<(Vec<u8>, Vec<u8>)>,
    do_parse!(
        tag!(b"diff --git ") >>
        names: alt!(
            do_parse!(
                name: map_opt!(quoted_name, trim_to_slash_inclusive) >>
                space >>
                other_name: map_opt!(file_name, trim_to_slash_inclusive) >>
                line_ending >>
                (name, other_name)
            ) |
            do_parse!(
				name: map_opt!(map!(take_until!("\""), |name| trim_right(name)), trim_to_slash_inclusive) >>
                other_name: map_opt!(quoted_name, trim_to_slash_inclusive) >>
                line_ending >>
                (name, other_name)
            ) |
            matching_name_pair
        ) >>
        (names)
    )
);

named!(
	file_name<Vec<u8>>,
	alt!(quoted_name | map!(not_line_ending, |slice| slice.into()))
);

named!(
    quoted_name<Vec<u8>>,
    delimited!(tag!(b"\""), unescape, tag!(b"\""))
);

named!(
    unescape<Vec<u8>>,
    escaped_transform!(
        not_quote_or_backslash,
        '\\',
        alt!(
			one_of!(r#""\"#) => { |ch| vec![ch as u8] } |
			tag!("a") => { |_| vec![b'\x07'] } |
			tag!("b") => { |_| vec![b'\x08'] } |
			tag!("n") => { |_| vec![b'\n'] } |
			tag!("r") => { |_| vec![b'\r'] } |
			tag!("t") => { |_| vec![b'\t'] } |
			tag!("v") => { |_| vec![b'\x0B'] } |
			octal_escape => { |byte| vec![byte] }
       )
    )
);

named!(not_quote_or_backslash, is_not!(r#""\"#));

named!(
    octal_escape<u8>,
    do_parse!(
        first_digit: one_of!("0123") >>
        second_digit: one_of!("01234567") >>
        third_digit: one_of!("01234567") >>
        (u8::from_str_radix(&vec![first_digit, second_digit, third_digit].into_iter().collect::<String>(), 8).unwrap())
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

fn matching_name_pair(input: &[u8]) -> IResult<&[u8], (Vec<u8>, Vec<u8>)> {
	let line_end = match input.iter().position(|item| *item == b'\n') {
		Some(position) => position,
		None => return Incomplete(Needed::Unknown)
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
					return Done(&input[line_end..], (name.clone(), name));
				}
			}
		}

		separator_start = separator_end + 1;
	}

	Error(ErrorKind::Custom(1)) // TODO: fall back to the first space as the separator
}

named!(
	patch_part<PatchPart<'a>>,
	alt!(
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
	index<PatchPart<'a>>,
	do_parse!(
		tag!("index ") >>
		hashes: separated_pair!(
			take_while_s!(is_hex_digit),
			tag!(".."),
			take_while_s!(is_hex_digit)
		) >>
		old_mode: opt!(preceded!(tag!(b" "), take_while!(is_oct_digit))) >>
		tag!("\n") >>
		(PatchPart::Index {
			old_index: hashes.0,
			new_index: hashes.1,
			old_mode: old_mode
		})
	)
);

named!(
	mode_change<PatchPart<'a>>,
	do_parse!(
		order: alt!(value!(Order::Old, tag!("old mode ")) | value!(Order::New, tag!("new mode "))) >>
		mode: take_while!(is_oct_digit) >>
		tag!("\n") >>
		(PatchPart::ModeChange(mode, order))
	)
);

named!(
	name_change<PatchPart<'a>>,
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
	presence_change<PatchPart<'a>>,
	do_parse!(
		change_type: alt!(value!(PresenceChangeType::Deleted, tag!("deleted file mode ")) | value!(PresenceChangeType::Added, tag!("new file mode "))) >>
		mode: take_while!(is_oct_digit) >>
		tag!("\n") >>
		(PatchPart::PresenceChange {
			change_type: change_type,
			mode: mode
		})
	)
);

named!(
	name<PatchPart<'a>>,
	do_parse!(
		order: alt!(value!(Order::Old, tag!("--- ")) | value!(Order::New, tag!("+++ "))) >>
		name: file_name >>
		tag!("\n") >>
		(PatchPart::Name(name, order))
	)
);

named!(
	similarity<PatchPart<'a>>,
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
	}

	if rest.len() > 0 {
		let (_, newline_absence_indicator_length) = try_parse!(rest, opt!( // Check for "\ No newline at end of file"
			do_parse!(
				tag!("\\") >>
				line: take_until!("\n") >>
				line_ending >>
				(line.len() + 2)
			)
		));

		if let Some(bytes_consumed) = newline_absence_indicator_length {
			bytes_consumed_total += bytes_consumed;
		}
	}

	Done(&input[bytes_consumed_total..], Hunk {
		old_file_range,
		new_file_range,
		data: &input[..bytes_consumed_total]
	})
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

#[cfg(test)]
mod test {
	use super::*;

	fn match_name(header: &[u8], expected_name: &[u8]) {
		let (name, other_name) = parse_header(header).to_result().unwrap();
		assert_eq!(name, other_name);
		assert_eq!(name.as_slice(), expected_name);
	}

	#[test]
	fn test_parse_header() {
		match_name(b"diff --git \"a/gradle.properties\" \"b/gradle.properties\"\n", b"gradle.properties");
		match_name(b"diff --git \"a/gradle.properties\" b/gradle.properties\n", b"gradle.properties");
		match_name(b"diff --git a/gradle.properties \"b/gradle.properties\"\n", b"gradle.properties");
		match_name(b"diff --git a/gradle.properties b/gradle.properties\n", b"gradle.properties");
	}

	#[test]
	fn test_unquote() {
		assert_eq!(quoted_name(br#""Test""#), Done(&b""[..], (&b"Test"[..]).into()));
		assert_eq!(quoted_name(br#""Te\\s\"t\n""#), Done(&b""[..], (&b"Te\\s\"t\n"[..]).into()));
		assert_eq!(quoted_name(br#""Test\040""#), Done(&b""[..], (&b"Test "[..]).into()));
	}

	#[test]
	fn test_hunk_header() {
		let header = b"@@ -14,4 +8,4 @@ org.gradle.jvmargs=-Xmx1536m\n";
		assert_eq!(hunk_header(header), Done(&b""[..], (14..18, 8..12)));
	}

	#[test]
	fn test_range() {
		assert_eq!(range(b"14,5 "), Done(&b" "[..], 14..19));
		assert_eq!(range(b"14 "), Done(&b" "[..], 14..15));
	}

	fn match_line(line: &[u8], old_file_lines_consumed: usize, new_file_lines_consumed: usize) {
		assert_eq!(hunk_line(line), Done(&b""[..], ((old_file_lines_consumed, new_file_lines_consumed), line.len())));
	}

	#[test]
	fn test_hunk_line() {
		match_line(b" # When configured, Gradle will run in incubating parallel mode.\n", 1, 1);
		match_line(b"-# org.gradle.parallel=true\n", 1, 0);
		match_line(b"+org.gradle.parallel=true\n", 0, 1);
	}

	#[test]
	fn test_digits_usize() {
		assert_eq!(digits_usize(b"14"), Done(&b""[..], 14));
	}

	#[test]
	fn test_hunk() {
		let hunk_data = br#"@@ -14,4 +8,4 @@ org.gradle.jvmargs=-Xmx1536m
 # When configured, Gradle will run in incubating parallel mode.
 # This option should only be used with decoupled projects. More details, visit
 # http://www.gradle.org/docs/current/userguide/multi_project_builds.html#sec:decoupled_projects
-# org.gradle.parallel=true
+org.gradle.parallel=true
"#;
		let expected = Hunk {
			old_file_range: 14..18,
			new_file_range: 8..12,
			data: &hunk_data[hunk_data.iter().position(|byte| *byte == b'\n').unwrap() + 1..]
		};
		assert_eq!(hunk(hunk_data), Done(&b""[..], expected))
	}
}
