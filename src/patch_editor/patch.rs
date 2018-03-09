use std::cmp::Ordering;
fn do_ranges_overlap(range: &Range<usize>, other_range: &Range<usize>) -> bool {
	range.start < other_range.end && range.end > other_range.start
}

fn check_overlaps(hunks: &[Hunk], other_hunks: &[Hunk]) -> bool {
	for hunk in hunks {
		let overlaps_found = other_hunks.iter().any(|&Hunk { ref old_file_range, ref new_file_range, .. }| {
			do_ranges_overlap(old_file_range, &hunk.old_file_range) || do_ranges_overlap(new_file_range, &hunk.new_file_range)
		});

		if overlaps_found {
			return true;
		}
	}

	return false;
}

pub struct OverlappingHunkError;

	pub fn new(change: Change, hunks: Vec<Hunk<'a>>) -> Result<Patch, OverlappingHunkError> {
		let mut sorted_hunks = hunks;
		sorted_hunks.sort_unstable();

		if check_overlaps(&sorted_hunks, &sorted_hunks) { return Err(OverlappingHunkError); }

		Ok(Patch {
			change,
			hunks: sorted_hunks,
		})
	}


	pub fn is_edit(&self) -> bool {
		match self.change {
			Change::Modification { modification_type: ModificationType::Edited, .. } => true,
			_ => false
		}
	}

	fn move_out_hunks(&mut self, positions: &[usize]) -> Vec<Hunk> {
		if !self.is_edit() {
			panic!("Only the edit patch can be changed. No addition, removal, mode change, or name change patches can be changed.");
		}

		let mut sorted_positions = positions.to_vec();
		sorted_positions.sort_unstable();

		sorted_positions
				.into_iter()
				.rev()
				.map(|position| self.hunks.remove(position))
				.rev()
				.collect()
	}

	pub fn move_out_hunks_into_patch(&mut self, positions: &[usize]) -> Patch {
		let change = self.change.clone();
		let hunks = self.move_out_hunks(positions);

		Patch {
			change,
			hunks,
		}
	}

	pub fn move_hunks_to(&'a mut self, positions: &[usize], patch: &mut Patch<'a>) -> Result<(), OverlappingHunkError> {
		let mut hunks = self.move_out_hunks(positions);

		if check_overlaps(&hunks, &patch.hunks) { return Err(OverlappingHunkError); }

		patch.hunks.append(&mut hunks);
		patch.hunks.sort_unstable();

		Ok(())
	}

	pub fn remove_hunks(&mut self, positions: &[usize]) {
		let mut sorted_positions = positions.to_vec();
		sorted_positions.sort_unstable();
		sorted_positions
				.into_iter()
				.rev()
				.for_each(|position| {
					self.hunks.remove(position);
				});
	}
#[derive(Debug, Eq, PartialEq, Clone)]
#[derive(Debug, Eq, PartialEq, Clone)]
#[derive(Debug, Eq, PartialEq, Clone)]
impl<'a> PartialOrd for Hunk<'a> {
	fn partial_cmp(&self, other: &Hunk<'a>) -> Option<Ordering> {
		Some(self.old_file_range.start.cmp(&other.old_file_range.start))
	}
}

impl<'a> Ord for Hunk<'a> {
	fn cmp(&self, other: &Self) -> Ordering {
		self.old_file_range.start.cmp(&other.old_file_range.start)
	}
}
