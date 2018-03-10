use std::process::Command;
use std::io;
use std::io::Read;
use std::process::Stdio;
use std::ffi::{OsStr, OsString};
use std::string::FromUtf8Error;

const COMMAND: &str = "git";

pub struct Git {
	repo_dir: OsString
}

impl Git { // TODO: detect detached head, apply patch, move branch
	pub fn new<S: AsRef<OsStr>>(repo_dir: S) -> Git {
		Git {
			repo_dir: repo_dir.as_ref().to_owned()
		}
	}

	fn prepare_command(&self) -> Command {
		let mut command = Command::new(COMMAND);
		command.arg("-C").arg(&self.repo_dir);
		command
	}

	fn run_command(command: &mut Command) -> Result<String, GitError> {
		let output = command.output()?;
		if !output.status.success() {
			Err(GitError::StatusError(output.status.code()))
		} else {
			Ok(String::from_utf8(output.stdout)?)
		}
	}

	pub fn checkout(&self, commit_spec: &str) -> Result<(), GitError> {
		let mut command = self.prepare_command();
		command.args(["checkout", "--force", commit_spec].iter());
		Git::run_command(&mut command)?;

		Ok(())
	}

	pub fn rev_list(&self, base_commit_spec: &str, merges_only: bool) -> Result<Vec<String>, GitError> {
		let mut command = self.prepare_command();
		command.args(&["rev-list", "--ancestry-path", &(String::from(base_commit_spec) + "..HEAD")]);
		if merges_only { command.arg("--merges"); }

		let output_text = Git::run_command(&mut command)?;
		Ok(output_text.split_terminator('\n')
				.map(|string| string.to_owned())
				.collect())
	}

	pub fn diff_tree(&self, commit_spec: &str) -> Result<String, GitError> {
		let mut command = self.prepare_command();
		command.args(&["diff-tree", "--no-commit-id", "--find-renames", "-p", "-r", commit_spec]);

		let output_text = Git::run_command(&mut command)?;
		Ok(output_text)
	}
}

#[derive(Debug)]
pub enum GitError {
	IoError(io::Error),
	OutputError(FromUtf8Error),
	StatusError(Option<i32>)
}

impl From<io::Error> for GitError {
	fn from(error: io::Error) -> Self {
		GitError::IoError(error)
	}
}

impl From<FromUtf8Error> for GitError {
	fn from(error: FromUtf8Error) -> Self {
		GitError::OutputError(error)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use std::path::PathBuf;
	use std::env::var;

	fn create_git() -> Git {
		let manifest_dir = var("CARGO_MANIFEST_DIR").unwrap();
		let test_resources_dir = [&manifest_dir, "resources", "tests"].iter().collect::<PathBuf>();

		let git = Git::new(test_resources_dir);
		git.checkout("reading-tests").unwrap();
		git
	}

	#[test]
	fn test_rev_list_merges_only() {
		let git = create_git();
		let result = git.rev_list("a23b1d79372e28779d364e98e3ca8d42050d4811", true).unwrap();
		assert_eq!(result, vec!["951534891c74c587db9f233763f5604724fa726f"]);
	}

	#[test]
	fn test_rev_list() {
		let expected = vec!["093b4b03ccb9a42846eb42f4b424c1020865693c",
			"551dbc06a60a500d745d2ed85027d46e46bdec15",
			"951534891c74c587db9f233763f5604724fa726f",
			"38eadc033cb1980d178052563c308377a4fe7e60",
			"fc3bf8af56bf2030d6e4c26182428e6f134aa2e2",
			"5b91d82043422d52dbe3fcd04b64a074af57675c",
			"96b7f6e6ad54bd54efc1a82bcd1c8dcdac63056d"];

		let git = create_git();
		let result = git.rev_list("a23b1d79372e28779d364e98e3ca8d42050d4811", false).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_diff_tree() {
		let expected = &"diff --git a/Test file 2.txt b/Test file 2.txt
index 60c340c..ec6c4de 100644
--- a/Test file 2.txt\t
+++ b/Test file 2.txt\t
@@ -1 +1 @@
-This is the second test file - modified
\\ No newline at end of file
+This is the second test file
\\ No newline at end of file
diff --git a/Test file.txt b/Test file.txt
index afe0cb3..9944a9f 100644
--- a/Test file.txt\t
+++ b/Test file.txt\t
@@ -1 +1 @@
-This is a test file - modified
\\ No newline at end of file
+This is a test file
\\ No newline at end of file
"[..];

		let git = create_git();
		let result = git.diff_tree("HEAD").unwrap();
		assert_eq!(result, expected);
	}
}