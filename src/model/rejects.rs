use std::ffi::{OsString, OsStr};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, read};
use std::iter::repeat;

use failure;
use pathdiff::diff_paths;

use model::{Model, View};
use patch_editor::patch::Hunk;
use patch_editor::parser::parse_rejects;
use std::fs::write;
use std::fs::remove_file;

#[derive(Clone)]
enum Command {
	ScanFiles,
	UpdateChangesToFile(Vec<u8>),
	SwitchToFile(usize),
	SwitchHunk(usize),
	AcceptHunk(usize, usize, bool),
	Reset(usize),
	SaveAndQuit
}

struct State {
	repo_dir_path: PathBuf,
	rejected_files: Vec<RejectedFile>,
	active_file: usize,
	active_hunk: usize
}

#[derive(Clone)]
struct RejectedFile {
	path: PathBuf,
	rejects_path: PathBuf,
	hunks: Vec<(Arc<Hunk>, bool)>,
	file_data: Vec<u8>,
	updated_file_data: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub struct RejectsModel {
	base: Model<Command>
}

impl RejectsModel {
	pub fn new<V: RejectsViewReceiver, S: AsRef<OsStr>>(view: Arc<V>, repo_dir: S) -> RejectsModel {
		let base = Model::new(view, repo_dir.as_ref().into(), move |repo_dir_owned: OsString| {
			Ok(State {
				repo_dir_path: repo_dir_owned.into(),
				rejected_files: Vec::new(),
				active_file: 0,
				active_hunk: 0
			})
		}, RejectsModel::perform_command);

		RejectsModel {
			base
		}
	}

	fn perform_command<V: RejectsViewReceiver>(view: &V, ref mut state: &mut State, command: Command) -> Result<(), failure::Error> {
		match command {
			Command::ScanFiles => {
				state.rejected_files = scan_directory(&state.repo_dir_path)?;
				state.active_file = 0;
				state.active_hunk = 0;

				RejectsModel::update_files_view(view, state);
				RejectsModel::update_hunks_and_file_data_view(view, state);
				RejectsModel::update_hunk_view(view, state);
			}
			Command::UpdateChangesToFile(updated_file_data) => {
				state.rejected_files[state.active_file].updated_file_data = Arc::new(updated_file_data);
			}
			Command::SwitchToFile(new_file_pos) => {
				if state.active_file != new_file_pos {
					state.active_file = new_file_pos;

					RejectsModel::update_hunks_and_file_data_view(view, state);
					RejectsModel::update_hunk_view(view, state);
				}
			}
			Command::SwitchHunk(hunk_pos) => {
				state.active_hunk = hunk_pos;
				RejectsModel::update_hunk_view(view, state);
			}
			Command::AcceptHunk(file_pos, hunk_pos, accepted) => {
				state.rejected_files[file_pos].hunks[hunk_pos].1 = accepted;

				if state.active_hunk < state.rejected_files[state.active_file].hunks.len() - 1 {
					state.active_hunk += 1;
				} else if state.active_file < state.rejected_files.len() - 1 {
					state.active_file += 1;
				}

				RejectsModel::update_files_view(view, state);
				RejectsModel::update_hunks_and_file_data_view(view, state);
				RejectsModel::update_hunk_view(view, state);
			}
			Command::Reset(file_pos) => {
				{
					let mut file = &mut state.rejected_files[file_pos];
					for (_, ref mut accepted) in &mut file.hunks {
						*accepted = false;
					}
					file.updated_file_data = Arc::new(file.file_data.clone());
				}

				state.active_file = file_pos;

				RejectsModel::update_hunks_and_file_data_view(view, state);
				RejectsModel::update_hunk_view(view, state);
			}
			Command::SaveAndQuit => {
				for file in &state.rejected_files {
					write(&file.path, &*file.updated_file_data)?;
				}

				for file in &state.rejected_files {
					remove_file(&file.rejects_path)?;
				}
				view.finish();
			}
		}
		Ok(())
	}

	fn update_files_view<V: RejectsViewReceiver>(view: &V, state: &State) {
		view.show_files(state.rejected_files.iter().map(|file| {
			let relative_path = diff_paths(&file.path, &state.repo_dir_path).unwrap().to_string_lossy().into_owned();
			let accepted = file.hunks.iter().all(|&(_, accepted)| accepted);
			(relative_path, accepted)
		}).collect());
	}

	fn update_hunks_and_file_data_view<V: RejectsViewReceiver>(view: &V, state: &State) {
		let active_file = &state.rejected_files[state.active_file];
		view.show_file_hunks(active_file.hunks.clone());
		view.show_file_data(active_file.updated_file_data.clone(), state.active_file);
	}

	fn update_hunk_view<V: RejectsViewReceiver>(view: &V, state: &State) {
		view.show_active_hunk(state.rejected_files[state.active_file].hunks[state.active_hunk].0.clone(), state.active_hunk);
	}

	pub fn scan_files(&self) {
		self.base.worker_sink.send(Command::ScanFiles).unwrap();
	}

	pub fn update_changes_to_current_file(&self, updated_file_data: Vec<u8>) {
		self.base.worker_sink.send(Command::UpdateChangesToFile(updated_file_data)).unwrap();
	}

	pub fn switch_to_file(&self, new_file_pos: usize) {
		self.base.worker_sink.send(Command::SwitchToFile(new_file_pos)).unwrap();
	}

	pub fn switch_to_hunk(&self, hunk_pos: usize) {
		self.base.worker_sink.send(Command::SwitchHunk(hunk_pos)).unwrap();
	}

	pub fn set_hunk_accepted(&self, file_pos: usize, hunk_pos: usize, accepted: bool) {
		self.base.worker_sink.send(Command::AcceptHunk(file_pos, hunk_pos, accepted)).unwrap();
	}

	pub fn reset(&self, file_pos: usize) {
		self.base.worker_sink.send(Command::Reset(file_pos)).unwrap();
	}

	pub fn save_and_quit(&self) {
		self.base.worker_sink.send(Command::SaveAndQuit).unwrap();
	}
}

fn scan_directory(dir_path: impl AsRef<Path>) -> Result<Vec<RejectedFile>, failure::Error> {
	let mut files = Vec::new();
	let dir_path = PathBuf::from(dir_path.as_ref());
	for entry in read_dir(&dir_path)? {
		let path = entry?.path();
		if path.is_dir() {
			files.extend(scan_directory(path)?);
		} else if path.is_file() {
			{
				let extension = match path.extension() {
					Some(extension) => extension,
					None => continue
				};

				if extension != "rej" {
					continue;
				}
			}

			let target_path = {
				let stem = match path.file_stem() {
					Some(stem) => stem,
					None => continue
				};
				dir_path.join(stem)
			};

			let file_data = if target_path.exists() {
				read(&target_path)?
			} else {
				Vec::new()
			};

			let rejects_data = read(&path)?;
			let hunks = parse_rejects(&rejects_data)?;
			let rejected_file = RejectedFile {
				path: target_path,
				rejects_path: path,
				hunks: hunks
						.into_iter()
						.map(|hunk| Arc::new(hunk)).into_iter().zip(repeat(false))
						.collect(),
				file_data: file_data.clone(),
				updated_file_data: Arc::new(file_data),
			};
			files.push(rejected_file);
		}
	}
	Ok(files)
}

pub trait RejectsViewReceiver: View {
	fn show_files(&self, files: Vec<(String, bool)>);
	fn show_file_hunks(&self, hunks: Vec<(Arc<Hunk>, bool)>);
	fn show_file_data(&self, data: Arc<Vec<u8>>, file_pos: usize);
	fn show_active_hunk(&self, hunk: Arc<Hunk>, hunk_pos: usize);
	fn finish(&self);
}