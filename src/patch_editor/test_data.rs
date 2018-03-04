use super::patch::{Change, FileProperties, Hunk, ModificationType, Patch};
pub const PATCH_ADDITION_DATA: &[u8] = br#"diff --git "a/ b/\320\235\320\276\320\262\321\213\320\271 \321\202\320\265\320\272\321\201\321\202\320\276\320\262\321\213\320\271 \320\264\320\276\320\272\321\203\320\274\320\265\320\275\321\202.txt" "b/ b/\320\235\320\276\320\262\321\213\320\271 \321\202\320\265\320\272\321\201\321\202\320\276\320\262\321\213\320\271 \320\264\320\276\320\272\321\203\320\274\320\265\320\275\321\202.txt"
new file mode 100644
index 0000000..e69de29
"#;

			change: Change::Modification {
				modification_type: ModificationType::Edited,
				old_properties: FileProperties {
					name: "gradle.properties".into(),
					mode: "100644".into(),
					index: "aac7c9b".into(),
				},
				new_properties: FileProperties {
					name: "gradle.properties".into(),
					mode: "100644".into(),
					index: "f33a6d7".into(),
				},
			hunks: vec![generate_hunk_1(), generate_hunk_2()],
		}
	};

	pub static ref PATCH_ADDITION: Patch<'static> = {
		Patch {
			change: Change::Addition {
				new_properties: FileProperties {
					name: " b/Новый текстовый документ.txt".into(),
					mode: "100644".into(),
					index: "e69de29".into(),
				}
			hunks: Vec::new(),
pub fn generate_hunk_2<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 14..18,
		new_file_range: 8..12,
		data: &*PATCH_DATA_HUNK_2_CONTENTS,
	}
}
