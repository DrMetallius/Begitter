use super::patch::{Change, FileProperties, Hunk, ModificationType, Patch};

pub const PATCH_DATA_HEADER: &[u8] = b"diff --git a/gradle.properties b/gradle.properties\n";
pub const PATCH_DATA_EXTENDED_HEADER: &[u8] = b"index aac7c9b..f33a6d7 100644\n";
pub const PATCH_DATA_NAMES: &[u8] = b"--- a/gradle.properties\n+++ b/gradle.properties\n";
pub const PATCH_DATA_HUNK_1_HEADER: &[u8] = b"@@ -1,9 +1,3 @@\n";
pub const PATCH_DATA_HUNK_1_CONTENTS: &[u8] = br"-# Project-wide Gradle settings.
-
-# IDE (e.g. Android Studio) users:
-# Gradle settings configured through the IDE *will override*
-# any settings specified in this file.
-
 # For more details on how to configure your build environment visit
 # http://www.gradle.org/docs/current/userguide/build_environment.html

";
pub const PATCH_DATA_HUNK_2_HEADER: &[u8] = b"@@ -14,4 +8,4 @@\n";
pub const PATCH_DATA_HUNK_2_OVERLAPPING_HEADER: &[u8] = b"@@ -8,4 +8,4 @@\n";
pub const PATCH_DATA_HUNK_2_CONTENTS: &[u8] = br" # When configured, Gradle will run in incubating parallel mode.
 # This option should only be used with decoupled projects. More details, visit
 # http://www.gradle.org/docs/current/userguide/multi_project_builds.html#sec:decoupled_projects
-# org.gradle.parallel=true
+org.gradle.parallel=true
";

pub const PATCH_ADDITION_DATA: &[u8] = br#"diff --git "a/ b/\320\235\320\276\320\262\321\213\320\271 \321\202\320\265\320\272\321\201\321\202\320\276\320\262\321\213\320\271 \320\264\320\276\320\272\321\203\320\274\320\265\320\275\321\202.txt" "b/ b/\320\235\320\276\320\262\321\213\320\271 \321\202\320\265\320\272\321\201\321\202\320\276\320\262\321\213\320\271 \320\264\320\276\320\272\321\203\320\274\320\265\320\275\321\202.txt"
new file mode 100644
index 0000000..e69de29
"#;

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

macro_rules! vec_from_slices {
	($($x:expr), *) => {
		{
			let mut vec = Vec::new();
	        $(
	            vec.extend_from_slice($x);
	        )*
	        vec
		}
	};
}

lazy_static! {
	pub static ref PATCH_DATA_HUNK_1: Vec<u8> = vec_from_slices![PATCH_DATA_HUNK_1_HEADER, PATCH_DATA_HUNK_1_CONTENTS];
	pub static ref PATCH_DATA_HUNK_2: Vec<u8> = vec_from_slices![PATCH_DATA_HUNK_2_HEADER, PATCH_DATA_HUNK_2_CONTENTS];

	pub static ref PATCH_DATA: Vec<u8> = generate_patch_data(false);
	pub static ref PATCH_DATA_NO_EXTENDED_HEADER: Vec<u8> = generate_patch_data(true);
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

	pub static ref COMBINED_PATCH_DATA: Vec<u8> = vec_from_slices![&*PATCH_DATA, &*PATCH_DATA_NO_NEW_LINES];

	pub static ref PATCH: Patch<'static> = {
		Patch {
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
			},
			hunks: Vec::new(),
		}
	};

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

	pub static ref COMBINED_PATCH: Vec<&'static Patch<'static>> = vec![&*PATCH, &*PATCH_NO_NEW_LINES];
}

pub fn generate_hunk_1<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 1..10,
		new_file_range: 1..4,
		data: &*PATCH_DATA_HUNK_1_CONTENTS,
	}
}

pub fn generate_hunk_2<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 14..18,
		new_file_range: 8..12,
		data: &*PATCH_DATA_HUNK_2_CONTENTS,
	}
}

fn generate_patch_data(no_extended_header: bool) -> Vec<u8> {
	vec_from_slices![PATCH_DATA_HEADER,
			if !no_extended_header { PATCH_DATA_EXTENDED_HEADER } else { &b""[..] },
			PATCH_DATA_NAMES,
			&**PATCH_DATA_HUNK_1,
			&**PATCH_DATA_HUNK_2]
}

fn generate_hunk_no_new_lines<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 1..2,
		new_file_range: 1..2,
		data: &*PATCH_DATA_NO_NEW_LINES_HUNK_CONTENTS
	}
}
