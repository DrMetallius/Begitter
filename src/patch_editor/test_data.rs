use super::patch::{FileProperties, Hunk, Operation, Patch};

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
pub const PATCH_DATA_HUNK_2_CONTENTS: &[u8] = br" # When configured, Gradle will run in incubating parallel mode.
 # This option should only be used with decoupled projects. More details, visit
 # http://www.gradle.org/docs/current/userguide/multi_project_builds.html#sec:decoupled_projects
-# org.gradle.parallel=true
+org.gradle.parallel=true
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

	pub static ref PATCH: Patch<'static> = {
		Patch {
			operation: Operation::Edited,
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
			similarity: None,
			hunks: vec![generate_hunk_1(), Hunk {
				old_file_range: 14..18,
				new_file_range: 8..12,
				data: &*PATCH_DATA_HUNK_2_CONTENTS,
			}],
		}
	};
}

pub fn generate_hunk_1<'a>() -> Hunk<'a> {
	Hunk {
		old_file_range: 1..10,
		new_file_range: 1..4,
		data: &*PATCH_DATA_HUNK_1_CONTENTS,
	}
}

fn generate_patch_data(no_extended_header: bool) -> Vec<u8> {
	vec_from_slices![PATCH_DATA_HEADER,
			if !no_extended_header { PATCH_DATA_EXTENDED_HEADER } else { &b""[..] },
			PATCH_DATA_NAMES,
			&**PATCH_DATA_HUNK_1,
			&**PATCH_DATA_HUNK_2]
}
