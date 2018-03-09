pub const PATCH_DATA_HUNK_2_OVERLAPPING_HEADER: &[u8] = b"@@ -8,4 +8,4 @@\n";
pub const PATCH_DATA_NO_NEW_LINES_HEADER: &[u8] = br"diff --git a/Test file 2.txt b/Test file 2.txt
index 60c340c..ec6c4de 100644
--- a/Test file 2.txt
+++ b/Test file 2.txt
";
pub const PATCH_DATA_NO_NEW_LINES_HUNK_HEADER: &[u8] = br"@@ -1 +1 @@
";
pub const PATCH_DATA_NO_NEW_LINES_HUNK_CONTENTS: &[u8] = br"-This is the second test file - modified
\ No newline at end of file
+This is the second test file
\ No newline at end of file
";

	pub static ref PATCH_DATA_OVERLAPPING_HUNKS: Vec<u8> = vec_from_slices![
			PATCH_DATA_HEADER,
			PATCH_DATA_NAMES,
			&**PATCH_DATA_HUNK_1,
			PATCH_DATA_HUNK_2_OVERLAPPING_HEADER,
			PATCH_DATA_HUNK_2_CONTENTS];

	pub static ref PATCH_DATA_NO_NEW_LINES: Vec<u8> = vec_from_slices![
		PATCH_DATA_NO_NEW_LINES_HEADER,
		PATCH_DATA_NO_NEW_LINES_HUNK_HEADER,
		PATCH_DATA_NO_NEW_LINES_HUNK_CONTENTS
	];

	pub static ref PATCH_NO_NEW_LINES: Patch<'static> = {
		Patch {
			change: Change::Modification {
				modification_type: ModificationType::Edited,
				old_properties: FileProperties {
					name: "Test file 2.txt".into(),
					mode: "100644".into(),
					index: "60c340c".into(),
				},
				new_properties: FileProperties {
					name: "Test file 2.txt".into(),
					mode: "100644".into(),
					index: "ec6c4de".into(),
				},
			},
			hunks: vec![generate_hunk_no_new_lines()],
		}
	};

fn generate_hunk_no_new_lines<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 1..2,
		new_file_range: 1..2,
		data: &*PATCH_DATA_NO_NEW_LINES_HUNK_CONTENTS
	}
}