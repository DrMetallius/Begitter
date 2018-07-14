mod parser;

use std::io::{Error, Write};
use std::collections::{HashSet, HashMap};
use std::borrow::Borrow;

use time::{self, Timespec};
use failure::{self, Backtrace};
use nom::ErrorKind;

use patch_editor::patch::{Patch, Change, ModificationType};

struct PatchClassification<T> {
	file_addition: HashMap<String, T>,
	file_removal_only: HashMap<String, T>,
	modification: HashMap<String, T>,
}

impl<T: Borrow<Patch>> PatchClassification<T> {
	fn classify(patch_iter: impl Iterator<Item=T>) -> PatchClassification<T> {
		let mut classification = PatchClassification {
			file_addition: HashMap::new(),
			file_removal_only: HashMap::new(),
			modification: HashMap::new(),
		};

		for patch in patch_iter {
			let (name, hash_map) = {
				if let Some(name) = get_added_file_name(patch.borrow()) {
					(name.clone(), &mut classification.file_addition)
				} else {
					if let Some(name) = get_removed_file_name(patch.borrow()) {
						(name.clone(), &mut classification.file_removal_only)
					} else {
						(get_modification_file_name(patch.borrow()).unwrap().clone(), &mut classification.modification)
					}
				}
			};
			hash_map.insert(name.clone(), patch);
		}

		classification
	}
}

fn get_added_file_name(patch: &Patch) -> Option<&String> {
	match patch.change {
		Change::Addition { ref new_properties } => Some(&new_properties.name),
		Change::Modification { ref modification_type, ref new_properties, .. } => match modification_type {
			ModificationType::Edited | ModificationType::ModeChanged => None,
			_ => Some(&new_properties.name)
		}
		Change::Removal { .. } => None
	}
}

fn get_removed_file_name(patch: &Patch) -> Option<&String> {
	match patch.change {
		Change::Addition { .. } => None,
		Change::Modification { ref modification_type, ref old_properties, .. } => match modification_type {
			ModificationType::Renamed { .. } => Some(&old_properties.name),
			_ => None
		}
		Change::Removal { ref old_properties } => Some(&old_properties.name),
	}
}

fn get_modification_file_name(patch: &Patch) -> Option<&String> {
	if let Change::Modification { ref modification_type, ref new_properties, .. } = patch.change {
		match modification_type {
			ModificationType::Edited | ModificationType::ModeChanged { .. } => return Some(&new_properties.name),
			_ => ()
		}
	}

	None
}

pub struct CombinedPatch {
	pub info: ChangeSetInfo,
	pub patches: Vec<Patch>,
}

impl CombinedPatch {
	pub fn write<W: Write>(&self, write: &mut W) -> Result<(), Error> {
		self.patches
				.iter()
				.map(|patch| patch.write(write))
				.collect()
	}

	pub fn absorb(&mut self, combined_patch: CombinedPatch) -> Result<(), failure::Error> {
		let (other_file_addition, other_file_removal_only, other_modification) = {
			let mut classification = PatchClassification::classify(self.patches.iter_mut());
			let mut other_classification = PatchClassification::classify(combined_patch.patches.into_iter());

			if classification.file_addition.keys().any(|key| other_classification.file_addition.contains_key(key)) {
				return Err(AbsorbtionError::ConflictingAdditions(Backtrace::new()).into());
			}

			// First do what we can with classification, the release it to operate on self.patches directly
			let mut other_unmerged_modification_patches = Vec::new();
			for (key, mut other_patch) in other_classification.modification {
				match classification.modification.get_mut(&key) {
					Some(patch) => {
						let positions = (0..other_patch.hunks.len()).into_iter().collect::<Vec<_>>();
						other_patch.move_hunks_to(&positions, patch)?
					}
					None => other_unmerged_modification_patches.push(other_patch)
				}
			}
			let other_unmerged_modification_patches = other_unmerged_modification_patches;

			let other_file_addition = other_classification.file_addition.into_iter().map(|(_, patch)| patch);

			let removed_files = classification.file_removal_only.keys().cloned().collect::<HashSet<String>>();
			let other_file_removal_patches = other_classification.file_removal_only
					.into_iter()
					.filter_map(|(key, patch)| if removed_files.contains(&key) { None } else { Some(patch) })
					.collect::<Vec<_>>();

			(other_file_addition, other_file_removal_patches, other_unmerged_modification_patches)
		};

		self.patches.extend(other_file_addition);
		self.patches.extend(other_file_removal_only);
		self.patches.extend(other_modification);

		Ok(())
	}
}

#[derive(Fail, Debug)]
pub enum AbsorbtionError {
	#[fail(display = "The same files were added in both combined patches")]
	ConflictingAdditions(Backtrace)
}

#[derive(Clone)]
pub struct Commit {
	pub hash: String,
	pub info: CommitInfo,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct CommitInfo {
	pub change_set_info: ChangeSetInfo,
	pub tree: String,
	pub parent: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ChangeSetInfo {
	pub author_action: PersonAction,
	pub committer_action: PersonAction,
	pub message: String,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct PersonAction {
	pub name: String,
	pub time: Timespec,
	pub time_zone: i32, // Offset in seconds from UTC
}

impl Default for PersonAction {
	fn default() -> PersonAction {
		PersonAction {
			name: String::new(),
			time: time::get_time(),
			time_zone: 0,
		}
	}
}

#[derive(Fail, Debug)]
#[fail(display = "Error when parsing the commit data: {:?}", _0)]
struct CommitParsingError(ErrorKind);

impl Commit {
	pub fn from_data(hash: String, commit_data: &[u8]) -> Result<Commit, failure::Error> {
		let result = parser::parse_commit_info(commit_data);
		let info = match result {
			Ok((_, info)) => info,
			Err(error) => return Err(CommitParsingError(error.into_error_kind()).into())
		};

		Ok(Commit {
			hash,
			info,
		})
	}
}