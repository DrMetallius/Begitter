use std::cmp::Ordering;
use std::ops::Range;
use std::io::Write;
use std::io::Error;
use std::borrow::{Borrow, Cow};

const FILE_NAME_PLACEHOLDER: &str = "/dev/null";

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

#[derive(Fail, Debug)]
#[fail(display = "Some hunks are overlapping")]
pub struct OverlappingHunkError;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Patch {
	pub change: Change,
	pub hunks: Vec<Hunk>,
}

impl Patch {
	// TODO: check if we actually need to forbid overlapping hunks
	pub fn new(change: Change, hunks: Vec<Hunk>) -> Result<Patch, OverlappingHunkError> {
		let mut sorted_hunks = hunks;
		sorted_hunks.sort_unstable();

		if check_overlaps(&sorted_hunks, &sorted_hunks) { return Err(OverlappingHunkError); }

		Ok(Patch {
			change,
			hunks: sorted_hunks,
		})
	}

	pub fn write<W: Write>(&self, write: &mut W) -> Result<(), Error> {
		let prefixed_old_name = match self.change {
			Change::Addition { .. } => FILE_NAME_PLACEHOLDER.into(),
			Change::Removal { ref old_properties } | Change::Modification { ref old_properties, .. } => String::from("a/") + &old_properties.name
		};
		let prefixed_escaped_old_name = format_name(&prefixed_old_name);

		let prefixed_new_name = match self.change {
			Change::Addition { ref new_properties } | Change::Modification { ref new_properties, .. } => String::from("b/") + &new_properties.name,
			Change::Removal { .. } => FILE_NAME_PLACEHOLDER.into(),
		};
		let prefixed_escaped_new_name = format_name(&prefixed_new_name);

		write.write_fmt(format_args!("diff --git {} {}\n", prefixed_escaped_old_name, prefixed_escaped_new_name))?;

		let operation_lines = match self.change {
			Change::Addition { ref new_properties } => {
				let mut operation_lines = format!("new file mode {}\n", new_properties.mode);
				if let Some(ref index) = new_properties.index {
					operation_lines.push_str(&format!("index 00000000..{}\n", index));
				}
				operation_lines
			}
			Change::Removal { ref old_properties } => {
				let mut operation_lines = format!("deleted file mode {}\n", old_properties.mode);
				if let Some(ref index) = old_properties.index {
					operation_lines.push_str(&format!("index {}..00000000 {}\n", index, old_properties.mode));
				}
				operation_lines
			}
			Change::Modification { ref modification_type, ref old_properties, ref new_properties } => {
				let mut operation_lines = match modification_type {
					&ModificationType::Edited => "".into(),
					&ModificationType::Copied { .. } => format!("copy from {}\ncopy to {}\n", format_name(&old_properties.name),
						format_name(&new_properties.name)),
					&ModificationType::Renamed { .. } => format!("rename from {}\nrename to {}\n", format_name(&old_properties.name),
						format_name(&new_properties.name)),
					&ModificationType::ModeChanged => format!("old mode {}\nnew mode {}\n", old_properties.mode, new_properties.mode)
				};

				if let Some(ref old_index) = old_properties.index {
					if let Some(ref new_index) = new_properties.index {
						operation_lines.push_str(&format!("index {}..{} {}\n", old_index, new_index, old_properties.mode));
					}
				}
				operation_lines
			}
		};

		write.write_all(operation_lines.as_bytes())?;
		write.write_fmt(format_args!("--- {}\n", prefixed_escaped_old_name))?;
		write.write_fmt(format_args!("+++ {}\n", prefixed_escaped_new_name))?;

		for hunk in &self.hunks {
			hunk.write(write)?;
		}

		Ok(())
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

		self.remove_indexes();

		let mut sorted_positions = positions.to_vec();
		sorted_positions.sort_unstable();

		let mut hunks = Vec::new();
		for position in sorted_positions.into_iter().rev() {
			hunks.insert(0, self.hunks.remove(position));
		}
		hunks
	}

	pub fn move_out_hunks_into_patch(&mut self, positions: &[usize]) -> Patch {
		self.remove_indexes();

		let change = self.change.clone();
		let hunks = self.move_out_hunks(positions);

		Patch {
			change,
			hunks,
		}
	}

	pub fn move_hunks_to(&mut self, positions: &[usize], patch: &mut Patch) -> Result<(), OverlappingHunkError> {
		let mut hunks = self.move_out_hunks(positions);

		if check_overlaps(&hunks, &patch.hunks) { return Err(OverlappingHunkError); }

		patch.remove_indexes();
		patch.hunks.append(&mut hunks);
		patch.hunks.sort_unstable();

		Ok(())
	}

	pub fn remove_hunks(&mut self, positions: &[usize]) {
		self.remove_indexes();

		let mut sorted_positions = positions.to_vec();
		sorted_positions.sort_unstable();
		sorted_positions
				.into_iter()
				.rev()
				.for_each(|position| {
					self.hunks.remove(position);
				});
	}

	fn remove_indexes(&mut self) {
		match self.change {
			Change::Addition { new_properties: FileProperties { ref mut index, .. } } | Change::Modification { new_properties: FileProperties { ref mut index, .. }, .. } => {
				*index = None
			}
			Change::Removal { .. } => ()
		}
	}
}

fn format_name(name: &str) -> Cow<str> {
	let escape = name.chars().any(|ch| ch.is_control() || ch == '"' || ch == '\\' || ch >= 0x80 as char);
	if !escape { return name.into(); }

	let mut buf = String::new();
	buf.push('"');

	let mut conversion_buf: [u8; 4] = Default::default();
	for ch in name.chars() {
		let replacement: Cow<str> = match ch {
			'\x07' => r"\a".into(),
			'\x08' => r"\b".into(),
			'\n' => r"\n".into(),
			'\r' => r"\r".into(),
			'\t' => r"\t".into(),
			'\x0B' => r"\v".into(),
			'\\' => r"\\".into(),
			'"' => r#"\""#.into(),
			_ if ch.is_control() || ch >= 0x80 as char => {
				let mut acc = String::new();
				for byte in ch.encode_utf8(&mut conversion_buf).as_bytes() {
					acc.push('\\');
					acc.push_str(&format!("{:03o})", byte));
				}
				acc.into()
			}
			_ => ch.encode_utf8(&mut conversion_buf).to_owned().into()
		};
		buf.push_str(replacement.borrow());
	}

	buf.push('"');

	return buf.into();
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Change {
	Addition {
		new_properties: FileProperties,
	},
	Removal {
		old_properties: FileProperties,
	},
	Modification {
		modification_type: ModificationType,
		old_properties: FileProperties,
		new_properties: FileProperties,
	},
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FileProperties {
	pub name: String,
	pub mode: String,
	pub index: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ModificationType {
	Copied { similarity: Option<u8> },
	Renamed { similarity: Option<u8> },
	ModeChanged,
	Edited,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Hunk {
	pub old_file_range: Range<usize>,
	pub new_file_range: Range<usize>,
	pub data: Vec<u8>,
}

fn range_to_str(range: &Range<usize>) -> String {
	let length = range.end - range.start;
	if length == 1 {
		format!("{}", range.start)
	} else {
		format!("{},{}", range.start, length)
	}
}

impl Hunk {
	fn write<W: Write>(&self, write: &mut W) -> Result<(), Error> {
		let old_file_range_str = range_to_str(&self.old_file_range);
		let new_file_range_str = range_to_str(&self.new_file_range);
		let header = format!("@@ -{} +{} @@\n", old_file_range_str, new_file_range_str);

		write.write_all(header.as_bytes())?;
		write.write_all(&self.data)?;

		Ok(())
	}
}

impl PartialOrd for Hunk {
	fn partial_cmp(&self, other: &Hunk) -> Option<Ordering> {
		Some(self.old_file_range.start.cmp(&other.old_file_range.start))
	}
}

impl<'a> Ord for Hunk {
	fn cmp(&self, other: &Self) -> Ordering {
		self.old_file_range.start.cmp(&other.old_file_range.start)
	}
}

#[cfg(test)]
mod test {
	use super::super::test_data::*;

	#[test]
	fn test_write_patch() {
		let mut buf = Vec::new();
		PATCH.write(&mut buf).unwrap();

		assert_eq!(&*buf, &**PATCH_DATA);

		let mut modified_patch = (*PATCH).clone();
		let mut other_patch = modified_patch.move_out_hunks_into_patch(&[0, 1]);
		other_patch.move_hunks_to(&[0, 1], &mut modified_patch).unwrap();

		buf.clear();
		modified_patch.write(&mut buf).unwrap();

		assert_eq!(&*buf, &**PATCH_DATA_NO_EXTENDED_HEADER);
	}

	#[test]
	fn test_write_hunk() {
		let mut buf = Vec::new();
		generate_hunk_1().write(&mut buf).unwrap();

		assert_eq!(&*buf, &**PATCH_DATA_HUNK_1);
	}
}