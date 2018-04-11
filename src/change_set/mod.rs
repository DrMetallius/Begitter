mod parser;

use time::{self, Timespec};
use patch_editor::patch::Patch;
use failure;

pub struct CombinedPatch {
	pub info: ChangeSetInfo,
	pub patches: Vec<Patch>
}

pub struct Commit {
	pub hash: String,
	pub info: ChangeSetInfo
}

#[derive(Eq, PartialEq, Debug)]
pub struct ChangeSetInfo {
	pub author_action: PersonAction,
	pub committer_action: PersonAction,
	pub message: String
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

impl Commit {
	pub fn from_data(hash: String, commit_data: &[u8]) -> Result<Commit, failure::Error> {
		let info = parser::parse_commit_info(commit_data).to_result()?;
		Ok(Commit {
			hash,
			info
		})
	}
}