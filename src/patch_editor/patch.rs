const FILE_NAME_PLACEHOLDER: &str = "/dev/null";

	pub change: Change,
		let prefixed_old_name = match self.change {
			Change::Addition { .. } => FILE_NAME_PLACEHOLDER.into(),
			Change::Removal { ref old_properties } | Change::Modification { ref old_properties, .. } => String::from("a/") + &old_properties.name
		};
		let prefixed_new_name = match self.change {
			Change::Addition { ref new_properties } | Change::Modification { ref new_properties, .. } => String::from("b/") + &new_properties.name,
			Change::Removal { .. } => FILE_NAME_PLACEHOLDER.into(),
		};
		let operation_data = match self.change {
			Change::Addition { ref new_properties } => Some(format!("new file mode {}\n", new_properties.mode)),
			Change::Removal { ref old_properties } => Some(format!("deleted file mode {}\n", old_properties.mode)),
			Change::Modification { ref modification_type, ref old_properties, ref new_properties } => {
				match modification_type {
					&ModificationType::Edited => None,
					&ModificationType::Copied { .. } => Some(format!("copy from {}\ncopy to {}\n", format_name(&old_properties.name),
						format_name(&new_properties.name))),
					&ModificationType::Renamed { .. } => Some(format!("rename from {}\nrename to {}\n", format_name(&old_properties.name),
						format_name(&new_properties.name))),
					&ModificationType::ModeChanged => Some(format!("old mode {}\nnew mode {}\n", old_properties.mode, new_properties.mode))
				}
			}
#[derive(Debug, Eq, PartialEq)]
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

pub enum ModificationType {
	Copied { similarity: Option<u8> },
	Renamed { similarity: Option<u8> },