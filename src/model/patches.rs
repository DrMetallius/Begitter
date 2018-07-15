use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use uuid::Uuid;
use failure::Backtrace;

use model::View;
use patch_editor::patch::{FileProperties, Change, ModificationType, OverlappingHunkError};
use change_set::{CombinedPatch, AbsorbtionError};

pub enum Direction {
	Left,
	Right,
}

struct PatchesModel<T: PatchesViewReceiver> {
	patches: HashMap<Uuid, CombinedPatch>,
	left: Side,
	right: Side,

	view: T,
}

#[derive(Copy, Clone)]
struct Side {
	selected_combined_patch: Uuid,
	selected_patch: usize,
}

impl<T: PatchesViewReceiver> PatchesModel<T> {
	fn is_simple_patch(&self, side: &Side) -> bool { // Involves only one file, not a rename, copy, or mode change
		let combined_patch = &self.patches[&side.selected_combined_patch];
		combined_patch.patches
				.iter()
				.all(|patch| {
					match patch.change {
						Change::Modification { ref modification_type, .. } => match modification_type {
							ModificationType::Edited { .. } => true,
							_ => false
						},
						_ => true
					}
				})
	}

	fn get_sides_by_direction(&self, direction: Direction) -> (Side, Side) {
		match direction {
			Direction::Left => (self.right, self.left),
			Direction::Right => (self.left, self.right)
		}
	}

	pub fn transfer_all_changes(&mut self, direction: Direction) {
		let (Side { selected_combined_patch: source, .. }, Side { selected_combined_patch: destination, .. }) = self.get_sides_by_direction(direction);
		let source_patch = self.patches.remove(&source).unwrap();

		let result = {
			let destination_patch = self.patches.get_mut(&destination).unwrap();
			destination_patch.absorb(source_patch)
		};

		match result {
			Err(err) => {
				let (err, original_patch) = err.into_patch();

				if let Some(combined_patch) = original_patch {
					self.patches.insert(source, combined_patch);
				}

				self.view.error(err.into());
			}
			_ => ()
		}
	}

	pub fn transfer_changes(&mut self, direction: Direction, patch_positions: &[usize]) {
		let (Side { selected_combined_patch: source, .. }, Side { selected_combined_patch: destination, .. }) = self.get_sides_by_direction(direction);
		let mut source_patch = self.patches.remove(&source).unwrap();

		let result = {
			let destination_patch = self.patches.get_mut(&destination).unwrap();
			source_patch.move_patches_to(patch_positions, destination_patch)
		};

		self.patches.insert(source, source_patch);
		match result {
			Err(err) => self.view.error(err.into()),
			_ => ()
		}
	}

	pub fn transfer_hunks(&mut self, direction: Direction, hunks: impl Iterator<Item=usize>) -> Result<(), HunkTransferringError> {
		let (Side { selected_combined_patch: source_id, selected_patch: source_patch_pos },
			Side { selected_combined_patch: destination_id, .. }) = self.get_sides_by_direction(direction);

		let mut source_combined_patch = self.patches.remove(&source_id).unwrap();
		let result = {
			let source_patch = &mut source_combined_patch.patches[source_patch_pos];
			let found_destination_patch = {
				let source_patch_file_name = match source_patch.get_edit_patch_file_name() {
					Some(name) => name,
					None => return Err(HunkTransferringError::SourcePatchIsNotModification)
				};

				let destination_combined_patch = self.patches.get_mut(&destination_id).unwrap();
				destination_combined_patch.patches.iter_mut().find(|patch| {
					patch.get_edit_patch_file_name().map_or(false, |name| name == source_patch_file_name)
				})
			};

			let destination_patch = match found_destination_patch {
				Some(patch) => patch,
				None => return Err(HunkTransferringError::DestinationPatchNotFoundOrNotModification)
			};

			source_patch.move_hunks_to(&hunks.collect::<Vec<_>>(), destination_patch) // TODO: can't we muff the original patch here? Check it
		};
		self.patches.insert(source_id, source_combined_patch);

		result?;
		Ok(())
	}
}

pub trait PatchesViewReceiver: View {}

#[derive(Fail, Debug)]
pub enum HunkTransferringError {
	SourcePatchIsNotModification,
	DestinationPatchNotFoundOrNotModification,
	OverlappingHunks(#[cause] OverlappingHunkError)
}

impl From<OverlappingHunkError> for HunkTransferringError {
	fn from(err: OverlappingHunkError) -> HunkTransferringError {
		HunkTransferringError::OverlappingHunks(err)
	}
}

impl Display for HunkTransferringError {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		let description = match self {
			HunkTransferringError::SourcePatchIsNotModification => "Can't modify patches which are not modifications",
			HunkTransferringError::DestinationPatchNotFoundOrNotModification => "Couldn't find a matching patch, or it was not a modification",
			HunkTransferringError::OverlappingHunks(_) => "Hunks in the patches are overlapping"
		};
		write!(f, "{}", description)
	}
}