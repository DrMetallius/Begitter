use std::collections::HashMap;

use uuid::Uuid;
use failure::Backtrace;

use change_set::CombinedPatch;
use patch_editor::patch::{FileProperties, Change, ModificationType};
use model::View;

pub enum VisibleChange {
	Addition {
		new_properties: FileProperties,
	},
	Removal {
		old_properties: FileProperties,
	},
	Modification {
		old_properties: FileProperties,
		new_properties: FileProperties,
	},
}

impl From<Change> for Vec<VisibleChange> {
	fn from(change: Change) -> Vec<VisibleChange> {
		match change {
			Change::Addition { new_properties } => vec![VisibleChange::Addition { new_properties }],
			Change::Removal { old_properties } => vec![VisibleChange::Removal { old_properties }],
			Change::Modification { modification_type, old_properties, new_properties } => match modification_type {
				ModificationType::Edited => vec![VisibleChange::Modification { old_properties, new_properties }],
				ModificationType::ModeChanged | ModificationType::Renamed { .. } => vec![VisibleChange::Removal { old_properties }, VisibleChange::Addition { new_properties }],
				ModificationType::Copied { .. } => vec![VisibleChange::Addition { new_properties }]
			}
		}
	}
}

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
	combined_patch: Uuid,
	visible_changes: Vec<VisibleChange>,
	selected_visible_change: usize,
}

impl<T: PatchesViewReceiver> PatchesModel<T> {
	pub fn transfer_all_changes(&mut self, direction: Direction) {
		let (source_side, destination_side) = match direction {
			Direction::Left => (&self.right, &self.left),
			Direction::Right => (&self.left, &self.right)
		};

		let source_patch = self.patches.remove(&source_side.combined_patch).unwrap();
		let destination_patch = self.patches.get_mut(&destination_side.combined_patch).unwrap();
		match destination_patch.absorb(source_patch) {
			Err(err) => self.view.error(err),
			_ => ()
		}
	}

	pub fn transfer_changes(&mut self, direction: Direction, visible_change_positions: &[usize]) {}

	pub fn transfer_hunks(&mut self, direction: Direction, visible_change_position: usize, hunks: impl Iterator<Item=usize>) {}
}

pub trait PatchesViewReceiver: View {}