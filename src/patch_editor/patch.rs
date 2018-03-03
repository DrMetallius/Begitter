use std::ops::Range;
use std::io::Write;
use std::io::Error;
use std::borrow::{Borrow, Cow};

#[derive(Debug, Eq, PartialEq)]
pub struct Patch<'a> {
	pub operation: Operation,
	pub old_properties: FileProperties,
	pub new_properties: FileProperties,
	pub similarity: Option<u8>,
	pub hunks: Vec<Hunk<'a>>,
}

impl<'a> Patch<'a> {
	pub fn write<W: Write>(&self, write: &mut W) -> Result<(), Error> {
		let prefixed_old_name = String::from("a/") + &self.old_properties.name;
		let prefixed_escaped_old_name = format_name(&prefixed_old_name);

		let prefixed_new_name = String::from("b/") + &self.new_properties.name;
		let prefixed_escaped_new_name = format_name(&prefixed_new_name);

		write.write_fmt(format_args!("diff --git {} {}\n", prefixed_escaped_old_name, prefixed_escaped_new_name))?;

		let operation_data = match self.operation {
			Operation::Edited => None,
			Operation::Added => Some(format!("new file mode {}\n", self.new_properties.mode)),
			Operation::Removed => Some(format!("deleted file mode {}\n", self.old_properties.mode)),
			Operation::Copied => Some(format!("copy from {}\ncopy to {}\n", format_name(&self.old_properties.name),
				format_name(&self.new_properties.name))),
			Operation::Renamed => Some(format!("rename from {}\nrename to {}\n", format_name(&self.old_properties.name),
				format_name(&self.new_properties.name))),
			Operation::ModeChanged => Some(format!("old mode {}\nnew mode {}\n", self.old_properties.mode, self.new_properties.mode))
		};

		if let Some(header_line) = operation_data {
			write.write_all(header_line.as_bytes())?;
		}

		write.write_fmt(format_args!("--- {}\n", prefixed_escaped_old_name))?;
		write.write_fmt(format_args!("+++ {}\n", prefixed_escaped_new_name))?;

		for hunk in &self.hunks {
			hunk.write(write)?;
		}

		Ok(())
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

#[derive(Debug, Eq, PartialEq)]
pub struct FileProperties {
	pub name: String,
	pub mode: String,
	pub index: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Operation {
	Added,
	Removed,
	Copied,
	Renamed,
	ModeChanged,
	Edited,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Hunk<'a> {
	pub old_file_range: Range<usize>,
	pub new_file_range: Range<usize>,
	pub data: &'a [u8],
}

fn range_to_str(range: &Range<usize>) -> String {
	let length = range.end - range.start;
	if length == 1 {
		format!("{}", range.start)
	} else {
		format!("{},{}", range.start, length)
	}
}

impl<'a> Hunk<'a> {
	fn write<W: Write>(&self, write: &mut W) -> Result<(), Error> {
		let old_file_range_str = range_to_str(&self.old_file_range);
		let new_file_range_str = range_to_str(&self.new_file_range);
		let header = format!("@@ -{} +{} @@\n", old_file_range_str, new_file_range_str);

		write.write_all(header.as_bytes())?;
		write.write_all(self.data)?;

		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use super::super::test_data::*;

	#[test]
	fn test_write_patch() {
		let mut buf = Vec::new();
		PATCH.write(&mut buf).unwrap();

		assert_eq!(&*buf, &**PATCH_DATA_NO_EXTENDED_HEADER);
	}

	#[test]
	fn test_write_hunk() {
		let mut buf = Vec::new();
		generate_hunk_1().write(&mut buf).unwrap();

		assert_eq!(&*buf, &**PATCH_DATA_HUNK_1);
	}