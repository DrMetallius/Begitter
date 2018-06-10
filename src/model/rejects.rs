use std::ffi::{OsString, OsStr};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, read};
use std::iter::repeat;

use failure;

use model::{Model, View}
use patch_editor::patch::Hunk;
use patch_editor::parser::parse_rejects;

#[derive(Clone)]
enum Command {
	ScanFiles,
	UpdateChangesToFileAndSwitch(usize, Vec<u8>, usize),
	AcceptHunk(usize, usize, Vec<u8>),
	Reset(usize)
}

struct State {
	repo_dir_path: PathBuf,
	rejected_files: Vec<RejectedFile>,
}

#[derive(Clone)]
struct RejectedFile {
	path: PathBuf,
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
			})
		}, RejectsModel::perform_command);

		let model = RejectsModel {
			base
		};
		model.scan_files();
		model
	}

	fn perform_command<V: RejectsViewReceiver>(view: &V, ref mut state: &mut State, command: Command) -> Result<(), failure::Error> {
		match command {
			Command::ScanFiles => {
				state.rejected_files = scan_directory(&state.repo_dir_path)?;
				RejectsModel::update_view(view, state, 0, true);
			}
			Command::UpdateChangesToFileAndSwitch(file_pos, updated_file_data, new_file_pos) => {
				state.rejected_files[file_pos].updated_file_data = Arc::new(updated_file_data);
				RejectsModel::update_view(view, state, new_file_pos, false);
			}
			Command::AcceptHunk(file_pos, hunk_pos, updated_file_data) => {
				{
					let mut file = &mut state.rejected_files[file_pos];
					file.hunks[hunk_pos].1 = true;
					file.updated_file_data = Arc::new(updated_file_data);
				}

				RejectsModel::update_view(view, &state, file_pos, false);
			}
			Command::Reset(file_pos) => {
				{
					let mut file = &mut state.rejected_files[file_pos];
					for (_, ref mut accepted) in &mut file.hunks {
						*accepted = false;
					}
					file.updated_file_data = Arc::new(file.file_data.clone());
				}

				RejectsModel::update_view(view, &state, file_pos, false);
			}
		}
		Ok(())
	}

	fn update_view<V: RejectsViewReceiver>(view: &V, state: &State, active_file_pos: usize, show_files: bool) {
		if show_files {
			view.show_files(state.rejected_files.iter().map(|file| {
				(file.path.to_string_lossy().into_owned(), file.hunks.iter().all(|&(_, merged)| merged))
			}).collect());
		}

		let active_file = &state.rejected_files[active_file_pos];
		view.show_file_hunks(active_file.hunks.clone());
		view.show_file_data(active_file.updated_file_data.clone());
	}

	fn scan_files(&self) {
		self.base.worker_sink.send(Command::ScanFiles).unwrap();
	}

	pub fn update_changes_to_file_and_switch(&self, file_pos: usize, updated_file_data: Vec<u8>, new_file_pos: usize) {
		self.base.worker_sink.send(Command::UpdateChangesToFileAndSwitch(file_pos, updated_file_data, new_file_pos)).unwrap();
	}

	pub fn accept_hunk(&self, file_pos: usize, hunk_pos: usize, updated_file_data: Vec<u8>) {
		self.base.worker_sink.send(Command::AcceptHunk(file_pos, hunk_pos, updated_file_data)).unwrap();
	}

	pub fn reset(&self, file_pos: usize) {
		self.base.worker_sink.send(Command::Reset(file_pos)).unwrap();
	}
}

fn scan_directory(path: impl AsRef<Path>) -> Result<Vec<RejectedFile>, failure::Error> {
	let mut files = Vec::new();
	for entry in read_dir(path)? {
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

			let file_data = read(&path)?;
			let hunks = parse_rejects(&file_data)?;
			let rejected_file = RejectedFile {
				path,
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
	fn show_file_data(&self, data: Arc<Vec<u8>>);
}