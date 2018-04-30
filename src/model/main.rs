use git::Git;
use std::ffi::OsStr;
use std::thread;
use std::sync;
use std::sync::Arc;
use std::ffi::OsString;
use failure;
use change_set::{Commit, CombinedPatch, ChangeSetInfo};
use patch_editor::parser::parse_combined_patch;

enum Command {
	GetBranches,
	ImportCommits(Vec<Commit>),
}

pub struct MainModel {
	worker_sink: sync::mpsc::Sender<Command>
}

impl MainModel {
	pub fn new<S: AsRef<OsStr>>(view: Arc<MainViewReceiver>, repo_dir: S) -> MainModel {
		let (sender, receiver) = sync::mpsc::channel();

		let model = MainModel {
			worker_sink: sender
		};

		let view_ref = view.clone();
		let repo_dir_owned: OsString = repo_dir.as_ref().into();
		thread::spawn(move || {
			let mut git = Git::new(repo_dir_owned);
			let mut combined_patches = Vec::<CombinedPatch>::new();
			loop {
				let command = match receiver.recv() {
					Ok(command) => command,
					Err(_) => break
				};
				let result = MainModel::perform_command(&*view_ref, &mut git, &mut combined_patches, command);
				if let Err(error) = result {
					view.error(error);
				}
			}
		});

		model.worker_sink.send(Command::GetBranches).unwrap();
		model
	}

	fn perform_command(view: &MainViewReceiver, git: &mut Git, combined_patches: &mut Vec<CombinedPatch>, command: Command) -> Result<(), failure::Error> {
		match command {
			Command::GetBranches => {
				let refs = git.show_refs_heads()?;
				let active = git.symbolic_ref("HEAD")?;
				view.show_branches(refs, active)?;

				let merges = git.rev_list(None, true)?;
				let commit_hashes = git.rev_list(if merges.is_empty() { None } else { Some(&merges[0]) }, false)?;

				let mut commits = Vec::<Commit>::new();
				for hash in commit_hashes {
					let commit_info_str = git.cat_file(&hash)?;
					let commit = Commit::from_data(hash, commit_info_str.as_bytes())?;
					commits.push(commit);
				}
				view.show_commits(commits)?;
				view.show_combined_patches(Vec::new())?;
			}
			Command::ImportCommits(commits) => {
				let mut new_combined_patches = Vec::<CombinedPatch>::new();
				for commit in commits {
					let combined_patch_data = git.diff_tree(&commit.hash)?;
					let patches = parse_combined_patch(combined_patch_data.as_bytes())?;
					let combined_patch = CombinedPatch {
						info: commit.info,
						patches
					};
					new_combined_patches.push(combined_patch);
				}

				combined_patches.extend(new_combined_patches);
				view.show_combined_patches(combined_patches.iter().map(|patch| patch.info.clone()).collect())?;
			}
		}
		Ok(())
	}

	pub fn import_commits(&self, commits: Vec<Commit>) {
		self.worker_sink.send(Command::ImportCommits(commits)).unwrap();
	}
}

pub trait MainViewReceiver: Sync + Send {
	fn error(&self, error: failure::Error);
	fn show_branches(&self, branches: Vec<String>, active_branch: String) -> Result<(), failure::Error>;
	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error>;
	fn show_combined_patches(&self, combined_patches: Vec<ChangeSetInfo>) -> Result<(), failure::Error>;
}