mod parser;

use time::{self, Timespec};
use patch_editor::patch::Patch;

pub struct ChangeSet {
	info: ChangeSetInfo,
	patches: Vec<Patch>
}

#[derive(Eq, PartialEq, Debug)]
pub struct ChangeSetInfo {
	hash: Option<String>,
	author_action: PersonAction,
	committer_action: PersonAction,
	message: String
}

#[derive(Eq, PartialEq, Debug)]
pub struct PersonAction {
	name: String,
	time: Timespec,
	time_zone: i32 // Offset in seconds from UTC
}

impl Default for PersonAction {
	fn default() -> PersonAction {
		PersonAction {
			name: String::new(),
			time: time::get_time(),
			time_zone: 0
		}
	}
}