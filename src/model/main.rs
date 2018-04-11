use git::Git;
use std::ffi::OsStr;
use std::thread;
use std::sync;
use std::sync::Arc;
use std::ffi::OsString;
use std::error::Error;
use failure;
use change_set::Commit;

enum Command {
	GetBranches
}

pub struct MainModel {
	worker_sink: sync::mpsc::Sender<Command>,
}

impl MainModel {
	pub fn new<S: AsRef<OsStr>>(view: Arc<MainView>, repo_dir: S) -> MainModel {
		let (sender, receiver) = sync::mpsc::channel();

		let model = MainModel {
			worker_sink: sender
		};

		let view_ref = view.clone();
		let repo_dir_owned: OsString = repo_dir.as_ref().into();
		thread::spawn(move || {
			let mut git = Git::new(repo_dir_owned);
			loop {
				let command = match receiver.recv() {
					Ok(command) => command,
					Err(_) => break
				};
				let result = MainModel::perform_command(&*view_ref, &mut git, command);
				if let Err(error) = result {
					view.error(error);
				}
			}
		});

		model.worker_sink.send(Command::GetBranches).unwrap();
		model
	}

	fn perform_command(view: &MainView, git: &mut Git, command: Command) -> Result<(), failure::Error> {
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
				view.show_edited_commits(&[])?;
			}
		}
		Ok(())
	}
}

pub trait MainView: Sync + Send {
	fn error(&self, error: failure::Error);
	fn show_branches(&self, branches: Vec<String>, active_branch: String) -> Result<(), failure::Error>;
	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error>;
	fn show_edited_commits(&self, commits: &[String]) -> Result<(), failure::Error>;
}