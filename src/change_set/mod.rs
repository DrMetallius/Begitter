mod parser;

use std::io::{Error, Write};
use std::collections::{HashSet, HashMap};
use std::borrow::Borrow;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt;

use time::{self, Timespec};
use failure;
use nom::ErrorKind;

use patch_editor::patch::{Patch, Change, ModificationType, OverlappingHunkError};

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

fn classification_into_original_patch(classification: PatchClassification<Patch>, original_patch_info: Option<ChangeSetInfo>) -> CombinedPatch {
	let patches = vec![classification.file_addition, classification.file_removal_only, classification.modification]
			.into_iter()
			.flat_map(|map| map.into_iter().map(|(_, patch)| patch).collect::<Vec<Patch>>())
			.collect();

	let info = if let Some(info) = original_patch_info {
		info
	} else {
		ChangeSetInfo::default()
	};

	CombinedPatch {
		info,
		patches,
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

#[derive(Debug, Clone)]
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

	pub fn absorb(&mut self, CombinedPatch { info, patches }: CombinedPatch) -> Result<(), AbsorbtionError> {
		self.absorb_patches(Some(info), patches.into_iter())
	}

	pub fn move_patches_to(&mut self, patch_positions: &[usize], combined_patch: &mut CombinedPatch) -> Result<(), AbsorbtionError> {
		let mut transferred_patches = Vec::new();
		for position in patch_positions.iter().rev() {
			transferred_patches.push(combined_patch.patches.remove(*position));
		}

		let result = self.absorb_patches(None, transferred_patches.into_iter());
		match result {
			Ok(_) => result,
			Err(err) => {
				let unprocessed_patches = err.combined_patch.unwrap().patches;
				combined_patch.patches.extend(unprocessed_patches.into_iter()); // If it becomes inconvenient, we could even restore the original order

				Err(AbsorbtionError {
					combined_patch: None,
					variant: err.variant,
				})
			}
		}
	}

	fn absorb_patches(&mut self, original_patch_info: Option<ChangeSetInfo>, patches: impl Iterator<Item=Patch>) -> Result<(), AbsorbtionError> {
		let (other_file_addition, other_file_removal_only, other_modification) = {
			let mut classification = PatchClassification::classify(self.patches.iter_mut());
			let mut other_classification = PatchClassification::classify(patches);

			if classification.file_addition.keys().any(|key| other_classification.file_addition.contains_key(key)) {
				let absorbtion_error = AbsorbtionError {
					combined_patch: Some(classification_into_original_patch(other_classification, original_patch_info)),
					variant: AbsorbtionErrorVariant::ConflictingAdditions,
				};
				return Err(absorbtion_error.into());
			}

			// First do what we can with classification, the release it to operate on self.patches directly
			let mut other_unmerged_modification_patches = Vec::new();

			for (key, mut other_patch) in other_classification.modification {
				match classification.modification.get_mut(&key) {
					Some(patch) => {
						let positions = (0..other_patch.hunks.len()).into_iter().collect::<Vec<_>>();
						other_patch.move_hunks_to(&positions, patch).unwrap(); // We'll remove the overlapping hunks check later, so this won't matter
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
pub struct AbsorbtionError {
	pub combined_patch: Option<CombinedPatch>,
	pub variant: AbsorbtionErrorVariant,
}

impl AbsorbtionError {
	pub fn into_patch(self) -> (AbsorbtionError, Option<CombinedPatch>) {
		let error = AbsorbtionError {
			combined_patch: None,
			variant: self.variant,
		};
		(error, self.combined_patch)
	}
}

impl Display for AbsorbtionError {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "Couldn't absorb patches as they contain conflicting additions")
	}
}

#[derive(Debug)]
pub enum AbsorbtionErrorVariant {
	ConflictingAdditions,
	HunkError(OverlappingHunkError),
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

impl Default for ChangeSetInfo {
	fn default() -> ChangeSetInfo {
		ChangeSetInfo {
			author_action: PersonAction::default(),
			committer_action: PersonAction::default(),
			message: "".into(),
		}
	}
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