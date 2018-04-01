use winapi::shared::minwindef::DWORD;
use begitter::git;
use begitter::git::Git;
use std::ffi::OsStr;
use std::thread;
use std::sync;
use std::sync::Arc;
use std::ffi::OsString;

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
					view.error();
				}
			}
		});

		model.worker_sink.send(Command::GetBranches);
		model
	}

	fn perform_command(view: &MainView, git: &mut Git, command: Command) -> git::Result<()> {
		match command {
			Command::GetBranches => {
				let refs = git.show_refs_heads()?;
				let active = git.symbolic_ref("HEAD")?;
				view.show_branches(refs, active);
			}
		}
		Ok(())
	}
}

pub trait MainView: Sync + Send {
	// TODO: add some sensible errors
	fn error(&self);
	fn show_branches(&self, branches: Vec<String>, active_branch: String) -> Result<(), DWORD>;
	fn show_commits(&self, commits: &[String]);
	fn show_edited_commits(&self, commits: &[String]);
	fn show_patches(&self, commits: &[String]);
}