use std::collections::HashMap;

use uuid::Uuid;
use failure::Backtrace;

use change_set::CombinedPatch;
use patch_editor::patch::{FileProperties, Change, ModificationType};
use model::View;
use change_set::AbsorbtionError;

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

struct Side {
	selected_combined_patch: Uuid,
	selected_patch: usize,
}

impl<T: PatchesViewReceiver> PatchesModel<T> {
	fn is_simple_patch(&self, side: &Side, visible_change_pos: usize) -> bool { // Involves only one file, not a rename, copy, or mode change
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

	fn get_selected_patches_ids(&self, direction: Direction) -> (Uuid, Uuid) {
		let (source_side, destination_side) = match direction {
			Direction::Left => (&self.right, &self.left),
			Direction::Right => (&self.left, &self.right)
		};
		(source_side.selected_combined_patch, destination_side.selected_combined_patch)
	}

	pub fn transfer_all_changes(&mut self, direction: Direction) {
		let (source, destination) = self.get_selected_patches_ids(direction);
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
			},
			_ => ()
		}
	}

	pub fn transfer_changes(&mut self, direction: Direction, patch_positions: &[usize]) {
		let (source, destination) = self.get_selected_patches_ids(direction);
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

	pub fn transfer_hunks(&mut self, direction: Direction, visible_change_position: usize, hunks: impl Iterator<Item=usize>) {}
}

pub trait PatchesViewReceiver: View {}