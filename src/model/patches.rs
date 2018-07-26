use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use uuid::{Uuid, UuidVersion};
use failure;

use model::View;
use patch_editor::patch::{Change, ModificationType, OverlappingHunkError};
use change_set::{AbsorbtionError, CombinedPatch};

macro_rules! check_presence {
	($($var:ident), *) => {
        $(
            let $var = $var.ok_or(HunkTransferringError::UnspecifiedSourceOrDestination)?;
        )*
	};
}

pub enum TargetSide {
	Left,
	Right,
}

#[derive(Copy, Clone)]
struct Side {
	selected_combined_patch: Option<Uuid>,
	selected_patch: Option<usize>,
}

impl Default for Side {
	fn default() -> Side {
		Side {
			selected_combined_patch: None,
			selected_patch: None,
		}
	}
}

pub struct PatchesModel<T: PatchesViewReceiver> {
	patches: HashMap<Uuid, CombinedPatch>,
	left: Side,
	right: Side,

	view: T,
}

impl<T: PatchesViewReceiver> PatchesModel<T> {
	pub fn new(view: T, patches: Vec<CombinedPatch>) -> PatchesModel<T> {
		let patches_map = patches.into_iter()
				.map(|patch| (Uuid::new(UuidVersion::Random).unwrap(), patch))
				.collect();

		let mut model = PatchesModel {
			patches: patches_map,
			left: Side::default(),
			right: Side::default(),
			view,
		};

		{
			let mut iter = model.patches.iter().take(2);

			fn side_from_iter<'a, 'b>(side: &mut Side, iter: &mut impl Iterator<Item=(&'a Uuid, &'b CombinedPatch)>) {
				if let Some((uuid, patch)) = iter.next() {
					side.selected_combined_patch = Some(*uuid);
					if patch.patches.len() > 0 {
						side.selected_patch = Some(0);
					}
				}
			}

			side_from_iter(&mut model.left, &mut iter);
			side_from_iter(&mut model.right, &mut iter);
		}

		model
	}

	pub fn initialize(&self) -> Result<(), failure::Error> {
		self.show_patches()
	}

	pub fn into_patches(self) -> Vec<CombinedPatch> {
		self.patches.into_iter().map(|(_, patch)| patch).collect()
	}

	fn get_sides_by_target_side(&self, direction: TargetSide) -> (Side, Side) {
		match direction {
			TargetSide::Left => (self.right, self.left),
			TargetSide::Right => (self.left, self.right)
		}
	}

	pub fn update_selection(&mut self, target_side: TargetSide, selection: Uuid) -> Result<(), failure::Error> {
		{
			let (side, other_side) = match target_side {
				TargetSide::Left => (&mut self.left, &mut self.right),
				TargetSide::Right => (&mut self.right, &mut self.left)
			};
			side.selected_combined_patch = Some(selection);

			if side.selected_combined_patch == other_side.selected_combined_patch {
				let other_patch = self.patches
						.iter()
						.map(|(&uuid, _)| uuid)
						.find(|&uuid| uuid != side.selected_combined_patch.unwrap());
				other_side.selected_combined_patch = other_patch;
			}
		}

		self.show_patches()
	}

	fn show_patches(&self) -> Result<(), failure::Error> {
		let mut entries = self.patches
				.iter()
				.map(|(&uuid, patch)| (uuid, patch.clone()))
				.collect::<Vec<_>>();
		entries.sort_by_key(|&(id, _)| id);

		self.view.view_combined_patches(entries, self.left.selected_combined_patch, self.right.selected_combined_patch)?;
		self.view.view_patches(self.left.selected_combined_patch, TargetSide::Left)?;
		self.view.view_patches(self.right.selected_combined_patch, TargetSide::Right)?;

		Ok(())
	}

	pub fn transfer_all_changes(&mut self, direction: TargetSide) -> Result<(), HunkTransferringError> {
		let (Side { selected_combined_patch: source, .. }, Side { selected_combined_patch: destination, .. }) = self.get_sides_by_target_side(direction);
		check_presence!(source, destination);

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

		Ok(())
	}

	pub fn transfer_changes(&mut self, direction: TargetSide, patch_positions: &[usize]) -> Result<(), HunkTransferringError> {
		let (Side { selected_combined_patch: source, .. }, Side { selected_combined_patch: destination, .. }) = self.get_sides_by_target_side(direction);
		check_presence!(source, destination);

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

		Ok(())
	}

	pub fn transfer_hunks(&mut self, direction: TargetSide, hunks: impl Iterator<Item=usize>) -> Result<(), HunkTransferringError> {
		let (Side { selected_combined_patch: source_id, selected_patch: source_patch_pos },
			Side { selected_combined_patch: destination_id, .. }) = self.get_sides_by_target_side(direction);
		check_presence!(source_id, source_patch_pos, destination_id);

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

pub trait PatchesViewReceiver: View {
	fn view_combined_patches(&self, patches: Vec<(Uuid, CombinedPatch)>, left_side_patch: Option<Uuid>, right_side_patch: Option<Uuid>) -> Result<(), failure::Error>;
	fn view_patches(&self, patch: Option<Uuid>, target_side: TargetSide) -> Result<(), failure::Error>;
}

#[derive(Fail, Debug)]
pub enum HunkTransferringError {
	SourcePatchIsNotModification,
	DestinationPatchNotFoundOrNotModification,
	OverlappingHunks(#[cause] OverlappingHunkError),
	UnspecifiedSourceOrDestination,
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
			HunkTransferringError::OverlappingHunks(_) => "Hunks in the patches are overlapping",
			HunkTransferringError::UnspecifiedSourceOrDestination => "Source and/or destination not specified, select patches and/or changes first"
		};
		write!(f, "{}", description)
	}
}