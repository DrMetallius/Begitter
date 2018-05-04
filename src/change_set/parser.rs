use super::ChangeSetInfo;
use nom::{IResult, IResult::Done, ErrorKind, FindSubstring, Slice, is_space, is_hex_digit, newline, rest};
use failure;
use change_set::{PersonAction, CommitInfo};
use time::Timespec;

const ERROR_INVALID_COMMITTER_OR_AUTHOR_INFO: u32 = 0;

named!(
	pub parse_commit_info<CommitInfo>,
	do_parse!(
		properties: many1!(property) >>
		newline >>
		message: rest >>
		change_set: expr_res!(change_set_info_from_properties(&properties, message)) >>
		(change_set)
	)
);

fn change_set_info_from_properties(properties: &[CommitProperty], message: &[u8]) -> Result<CommitInfo, failure::Error> {
	let mut commit_info = CommitInfo {
		change_set_info: ChangeSetInfo {
			author_action: PersonAction::default(),
			committer_action: PersonAction::default(),
			message: String::from_utf8(message.into())?,
		},
		tree: String::default(),
		parent: None,
	};

	fn person_action_from_property(name: &[u8], time: i64, time_zone: i32) -> Result<PersonAction, failure::Error> {
		Ok(PersonAction {
			name: String::from_utf8(name.into())?,
			time: Timespec { sec: time, nsec: 0 },
			time_zone,
		})
	}

	for property in properties {
		match property {
			&CommitProperty::Tree(tree) => commit_info.tree = String::from_utf8(tree.into())?,
			&CommitProperty::Parent(parent) => commit_info.parent = Some(String::from_utf8(parent.into())?),
			&CommitProperty::Author(name, time, time_zone) => commit_info.change_set_info.author_action = person_action_from_property(name, time, time_zone)?,
			&CommitProperty::Committer(name, time, time_zone) => commit_info.change_set_info.committer_action = person_action_from_property(name, time, time_zone)?
		}
	}

	Ok(commit_info)
}

enum CommitProperty<'a> {
	Tree(&'a [u8]),
	Parent(&'a [u8]),
	Author(&'a [u8], i64, i32),
	Committer(&'a [u8], i64, i32),
}

named!(property<CommitProperty>, alt!(
	tree |
	parent |
	author_or_committer
));

named!(tree<CommitProperty>, map!(delimited!(tag!("tree "), take_while1_s!(is_hex_digit), newline), |hash| CommitProperty::Tree(hash)));

named!(parent<CommitProperty>, map!(delimited!(tag!("parent "), take_while1_s!(is_hex_digit), newline), |hash| CommitProperty::Parent(hash)));

fn author_or_committer(input: &[u8]) -> IResult<&[u8], CommitProperty> {
	let (rest, line) = try_parse!(input, take_until_and_consume1!(&b"\n"[..]));
	let parts: Vec<&[u8]> = line.rsplitn(3, |&ch| is_space(ch)).collect();
	if parts.len() < 3 {
		return IResult::Error(ErrorKind::Custom(ERROR_INVALID_COMMITTER_OR_AUTHOR_INFO));
	}

	let (_, time) = try_parse!(parts[1], parse_to!(u64));
	let (_, time_zone) = try_parse!(parts[0], parse_to!(i32));

	let header_and_name: Vec<&[u8]> = parts[2].splitn(2, |&ch| is_space(ch)).collect();
	if header_and_name.len() < 2 {
		return IResult::Error(ErrorKind::Custom(ERROR_INVALID_COMMITTER_OR_AUTHOR_INFO));
	}

	match header_and_name[0] {
		header if header == &b"author"[..] => Done(rest, CommitProperty::Author(header_and_name[1], time, time_zone)),
		header if header == &b"committer"[..] => Done(rest, CommitProperty::Committer(header_and_name[1], time, time_zone)),
		_ => IResult::Error(ErrorKind::Custom(ERROR_INVALID_COMMITTER_OR_AUTHOR_INFO))
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_parse_commit() {
		let data = r"tree 90f8bfa9fb9053b2004907c50c8cf57a31ea6aed
parent 6f522f142a4fa563b871796fad4d46f822745cf3
author Один чувак <абырвалг@example.com> 1523207666 +0300
committer Alexander Gazarov <drmetallius@gmail.com> 1523207822 +0300

Это проверка
".as_bytes();
		let result = parse_commit_info(data).to_result().unwrap();
		assert_eq!(result, CommitInfo {
			change_set_info: ChangeSetInfo {
				author_action: PersonAction {
					name: String::from("Один чувак <абырвалг@example.com>"),
					time: Timespec { sec: 1523207666, nsec: 0 },
					time_zone: 300,
				},
				committer_action: PersonAction {
					name: String::from("Alexander Gazarov <drmetallius@gmail.com>"),
					time: Timespec { sec: 1523207822, nsec: 0 },
					time_zone: 300,
				},
				message: String::from("Это проверка\n"),
			},
			tree: String::from("90f8bfa9fb9053b2004907c50c8cf57a31ea6aed"),
			parent: Some(String::from("6f522f142a4fa563b871796fad4d46f822745cf3"))
		});
	}
}