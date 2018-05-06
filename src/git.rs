use std::process::Command;
use std::io::{self, Write};
use std::process::{Output, Stdio};
use std::ffi::{OsStr, OsString};
use std::string::FromUtf8Error;
use std::vec::Vec;

use failure::Backtrace;

use nom::ErrorKind;

use super::parsing_utils::file_name;

#[cfg(windows)]
use {
	std::os::windows::process::CommandExt,
	winapi::um::winbase::CREATE_NO_WINDOW
};

const COMMAND: &str = "git";
const STATUS_PORCELAIN_V2_COLUMNS: usize = 11;

pub const BRANCH_PREFIX: &str = "refs/heads/";

pub type Result<T> = ::std::result::Result<T, GitError>;

pub struct Git {
	repo_dir: OsString
}

impl Git {
	pub fn new<S: AsRef<OsStr>>(repo_dir: S) -> Git {
		Git {
			repo_dir: repo_dir.as_ref().to_owned()
		}
	}

	fn prepare_command<I, S>(&self, args: I) -> Command
		where I: IntoIterator<Item=S>, S: AsRef<OsStr> {
		let mut command = Command::new(COMMAND);
		command.arg("-C")
			.arg(&self.repo_dir)
			.args(args);
		if cfg!(windows) {
			command.creation_flags(CREATE_NO_WINDOW);
		}
		command
	}

	fn read_command_output(output: Output) -> Result<String> {
		if !output.status.success() {
			Err(GitError::StatusError(output.status.code(), String::from_utf8(output.stderr).unwrap_or("".into()), Backtrace::new()))
		} else {
			Ok(String::from_utf8(output.stdout)?)
		}
	}

	fn run_command<I, S>(&self, args: I) -> Result<String>
		where I: IntoIterator<Item=S>, S: AsRef<OsStr> {
		let output = self.prepare_command(args).output()?;
		Git::read_command_output(output)
	}

	fn run_command_with_stdin<I, S>(&self, args: I, stdin_data: &[u8]) -> Result<String>
		where I: IntoIterator<Item=S>, S: AsRef<OsStr> {
		let mut child = self.prepare_command(args)
				.stdin(Stdio::piped())
				.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.spawn()?;

		{
			let stdin = child.stdin.as_mut();
			stdin.unwrap().write_all(stdin_data)?;
		}

		let output = child.wait_with_output()?;
		Git::read_command_output(output)
	}

	fn parse_name(name_data: &[u8]) -> Result<String> {
		match file_name(name_data).to_result() {
			Ok(data) => String::from_utf8(data).map_err(|err| err.into()),
			Err(cause) => Err(cause.into())
		}
	}

	pub fn cat_file(&self, spec: &str) -> Result<String> {
		self.run_command(&["cat-file", "-p", spec])
	}

	pub fn status_conflicts(&self) -> Result<Vec<String>> {
		let output = self.run_command(&["status", "--porcelain=v2"])?;
		let conflicting_files = output.split_terminator('\n')
				.filter(|string| string.starts_with("u "))
				.map(|string| {
					let mut name = string;
					for _ in 0..STATUS_PORCELAIN_V2_COLUMNS - 1 {
						name = &name[name.find(' ').unwrap() + 1..];
					}

					Git::parse_name(name.as_bytes())
				})
				.collect::<Result<Vec<String>>>()?;
		Ok(conflicting_files)
	}

	pub fn show_ref(&self, ref_name: &str) -> Result<String> {
		let mut args = vec!["show-ref", "--hash"];
		if ref_name == "HEAD" {
			args.push("--head");
		}
		args.push(ref_name);

		let result = self.run_command(&args)?;
		Ok(result.trim().into())
	}

	pub fn show_refs_heads(&self) -> Result<Vec<String>> {
		let result = self.run_command(["show-ref", "--heads"].into_iter())?;
		let heads_opt = result.split_terminator('\n')
				.map(|line| line.split(' ').nth(1).map(|string| string.into()))
				.collect::<Option<Vec<_>>>();
		match heads_opt {
			Some(heads) => Ok(heads),
			None => Err(GitError::from(ErrorKind::Custom(0)))
		}
	}

	pub fn rev_list(&self, base_commit_spec: Option<&str>, merges_only: bool) -> Result<Vec<String>> {
		let range;
		let mut args = vec!["rev-list"];
		if merges_only {
			args.push("--merges");
		}

		match base_commit_spec {
			Some(base_commit_spec) => {
				range = String::from(base_commit_spec) + "..HEAD";
				args.push("--ancestry-path");
				args.push(&range);
			},
			None => args.push("HEAD")
		}

		let output_text = self.run_command(args)?;
		Ok(output_text.split_terminator('\n')
				.map(|string| string.to_owned())
				.collect())
	}

	pub fn symbolic_ref(&self, ref_name: &str) -> Result<String> {
		let result = self.run_command(&["symbolic-ref", "--quiet", ref_name])?;
		Ok(result.trim().into())
	}

	pub fn symbolic_ref_update(&self, ref_name: &str, target: &str) -> Result<String> {
		self.run_command(&["symbolic-ref", "--quiet", ref_name, target])
	}

	pub fn update_ref(&self, ref_name: &str, object_sha: &str) -> Result<()> { // Can only accept actual objects, refs are no good
		self.run_command(&["update-ref", "--no-deref", ref_name, object_sha])?;
		Ok(())
	}

	pub fn diff_tree(&self, commit_spec: &str) -> Result<String> {
		self.run_command(&["diff-tree", "--no-commit-id", "--find-renames", "--patch", "-r", commit_spec])
	}

	pub fn diff_index_names(&self, commit_spec: &str) -> Result<Vec<String>> {
		let output_text = self.run_command(&["diff-index", "--cached", "--name-only", commit_spec])?;
		output_text.split_terminator('\n')
				.map(|string| Git::parse_name(string.as_bytes()))
				.collect()
	}

	pub fn read_tree<S: AsRef<str>>(&self, commit_spec: Option<S>) -> Result<()> {
		let target = commit_spec.as_ref().map_or("--empty", |spec| spec.as_ref() );
		self.run_command(&["read-tree", target])?;
		Ok(())
	}

	pub fn checkout_index(&self) -> Result<()> {
		// --index is required for the index to match what's in the working dir
		self.run_command(&["checkout-index", "--all", "--force", "--index"])?;
		Ok(())
	}

	pub fn apply(&self, patch: &[u8], working_tree: bool) -> Result<()> {
		let mut args = vec!["apply"];

		if working_tree {
			args.push("--index");
			args.push("--3way");
		} else {
			args.push("--cached");
		}

		args.push("-");

		self.run_command_with_stdin(args, patch)?;
		Ok(())
	}

	pub fn update_index<I, S>(&self, files: I) -> Result<()>
		where I: IntoIterator<Item=S>, S: AsRef<OsStr> {
		let mut args: Vec<OsString> = vec!["update-index".into(), "--".into()];
		for ref entry in files {
			args.push(entry.into());
		}

		self.run_command(&args)?;
		Ok(())
	}

	pub fn write_tree(&self) -> Result<String> {
		let tree = self.run_command(&["write-tree"])?;
		Ok(tree.trim().into())
	}

	pub fn commit_tree<S: AsRef<str>>(&self, tree: &str, parent: Option<S>, message: &str) -> Result<String> { // TODO: add author and committer
		let mut args = vec!["commit-tree", tree, "-m", message];
		if let Some(parent_spec) = parent.as_ref() {
			args.push("-p");
			args.push(parent_spec.as_ref());
		}
		let commit = self.run_command(args)?;

		Ok(commit.trim().into())
	}
}

#[derive(Fail, Debug)]
pub enum GitError {
	#[fail(display = "I/O error: {}", _0)]
	IoError(io::Error, Backtrace),
	#[fail(display = "Error when parsing Unicode input: {}", _0)]
	EncodingError(FromUtf8Error, Backtrace),
	#[fail(display = "Error when parsing the patch data: {}", _0)]
	ParsingError(ErrorKind, Backtrace),
	#[fail(display = "Git failure, status {:?}: {}", _0, _1)]
	StatusError(Option<i32>, String, Backtrace)
}

impl GitError {
	pub fn to_status(&self) -> Option<i32> {
		match *self {
			GitError::StatusError(status, _, _) => status.clone(),
			_ => None
		}
	}
}

impl From<io::Error> for GitError {
	fn from(error: io::Error) -> Self {
		GitError::IoError(error, Backtrace::new())
	}
}

impl From<FromUtf8Error> for GitError {
	fn from(error: FromUtf8Error) -> Self {
		GitError::EncodingError(error, Backtrace::new())
	}
}

impl From<ErrorKind> for GitError {
	fn from(error: ErrorKind) -> Self {
		GitError::ParsingError(error, Backtrace::new())
	}
}

#[cfg(test)]
mod test { // TODO: undo marking variables as unused, this breaks stuff
	use super::*;
	use std::path::{Path, PathBuf};
	use std::env::var;
	use std::fs::{copy, create_dir, File, read_dir};
	use tempdir::TempDir;

	const PATCH: &[u8] = br"diff --git a/Test file.txt b/Test file.txt
index 9944a9f..e9459b0 100644
--- a/Test file.txt
+++ b/Test file.txt
@@ -1 +1 @@
-This is a test file
\ No newline at end of file
+This is just a test file
\ No newline at end of file
";

	fn copy_dir<P: AsRef<Path>, Q: AsRef<Path>>(source: P, destination: Q) {
		for entry in read_dir(source).unwrap() {
			let entry = entry.unwrap();
			let path = entry.path();

			let mut new_destination = PathBuf::from(destination.as_ref());
			new_destination.push(path.file_name().unwrap());

			if path.is_dir() {
				create_dir(&new_destination).unwrap();
				copy_dir(path, new_destination);
			} else {
				copy(path, new_destination).unwrap();
			}
		}
	}

	fn create_git() -> (Git, TempDir) {
		let temp_dir = TempDir::new("begitter").unwrap();

		let manifest_dir = var("CARGO_MANIFEST_DIR").unwrap();
		let test_resources_dir = [&manifest_dir, "resources", "tests"].iter().collect::<PathBuf>();

		copy_dir(test_resources_dir, temp_dir.path());

		let git = Git::new(temp_dir.path());

		let target_commit = git.show_ref("reading-tests").unwrap();
		git.update_ref("HEAD", &target_commit).unwrap();
		
		(git, temp_dir)
	}

	fn apply_patch_with_conflicts(git: &Git) {
		let target_commit = git.show_ref("conflict-tests").unwrap();
		git.update_ref("HEAD", &target_commit).unwrap();
		git.read_tree(Some("refs/tags/conflict-tests")).unwrap();
		git.checkout_index().unwrap();

		let apply_result = git.apply(PATCH, true);
		assert!(apply_result.is_err());
	}

	#[test]
	fn test_rev_list_merges_only() {
		let (git, _temp_dir) = create_git();
		let result = git.rev_list(Some("a23b1d79372e28779d364e98e3ca8d42050d4811"), true).unwrap();
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

		let (git, _temp_dir) = create_git();
		let result = git.rev_list(Some("a23b1d79372e28779d364e98e3ca8d42050d4811"), false).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_symbolic_ref() {
		let (git, _temp_dir) = create_git();

		let result = git.symbolic_ref("HEAD");
		match result {
			Err(GitError::StatusError(Some(1), _, _)) => (),
			other => panic!("Symbolic ref is supposed to exit with status 1 when in a detached head state, was {:?}", other)
		}

		git.symbolic_ref_update("HEAD", "refs/heads/test-branch").unwrap();
		assert_eq!("refs/heads/test-branch", git.symbolic_ref("HEAD").unwrap());
		assert_eq!("6f522f142a4fa563b871796fad4d46f822745cf3", git.show_ref("HEAD").unwrap());
	}

	#[test]
	fn test_read_tree() {
		let (git, _temp_dir) = create_git();

		git.read_tree(Some("refs/heads/test-branch")).unwrap();
		assert!(git.diff_index_names("refs/heads/test-branch").unwrap().is_empty());

		git.read_tree(Some("refs/tags/reading-tests")).unwrap();
		assert!(!git.diff_index_names("refs/heads/test-branch").unwrap().is_empty());
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

		let (git, _temp_dir) = create_git();
		let result = git.diff_tree("HEAD").unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_apply() {
		let (git, dir) = create_git();
		git.read_tree(Some("refs/tags/reading-tests")).unwrap();
		git.apply(PATCH, false).unwrap();

		assert!(!git.diff_index_names("refs/tags/reading-tests").unwrap().is_empty());
	}

	#[test]
	fn test_status_conflicts() {
		let (git, _temp_dir) = create_git();
		apply_patch_with_conflicts(&git);

		assert_eq!(vec!["Test file.txt"], git.status_conflicts().unwrap());
	}

	#[test]
	fn test_update_index() {
		let (git, temp_dir) = create_git();
		apply_patch_with_conflicts(&git);

		let mut test_file_path = temp_dir.path().to_owned();
		test_file_path.push("Test file.txt");

		{
			let mut file = File::create(test_file_path,).unwrap();
			file.write_all(b"This is just a test file\n").unwrap();
		};

		git.update_index(&["Test file.txt"]).unwrap();
		assert_eq!(<Vec<String>>::new(), git.status_conflicts().unwrap());
	}

	#[test]
	fn test_write_tree_and_commit() {
		let (git, _temp_dir) = create_git();

		let target_commit = git.show_ref("conflict-tests").unwrap();
		git.update_ref("HEAD", &target_commit).unwrap();
		git.read_tree(Some("refs/tags/conflict-tests")).unwrap();

		let tree = git.write_tree().unwrap();
		git.commit_tree(&tree, Some(&target_commit), "Test commit").unwrap();
	}
}