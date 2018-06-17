use std::ffi::OsStr;
use std::sync::Arc;
use std::ffi::OsString;

use failure::{self, Backtrace};

use git::{self, Git};
use change_set::{Commit, CombinedPatch, ChangeSetInfo};
use patch_editor::parser::parse_combined_patch;
use std::collections::HashMap;
use model::{Model, View};

#[derive(Clone)]
enum Command {
	GetBranches,
	ImportCommits(Vec<Commit>),
	SetPatchMessage(usize, String),
	MovePatch(usize, usize),
	DeletePatch(usize),
	ApplyCommits(Commit),
	SwitchToBranch(String),
}

struct State {
	git: Git,
	combined_patches: Vec<CombinedPatch>,
}

#[derive(Clone)]
pub struct MainModel {
	base: Model<Command>
}

impl MainModel {
	pub fn new<V: MainViewReceiver, S: AsRef<OsStr>>(view: Arc<V>, repo_dir: S) -> MainModel {
		let base = Model::new(view, repo_dir.as_ref().into(), move |repo_dir_owned: OsString| {
			Ok(State {
				git: Git::new(repo_dir_owned),
				combined_patches: Vec::new(),
			})
		}, MainModel::perform_command);

		let model = MainModel {
			base
		};
		model.base.send(Command::GetBranches);
		model
	}

	fn perform_command<V: MainViewReceiver>(view: &V, ref mut state: &mut State, command: Command) -> Result<(), failure::Error> {
		fn show_combined_patches<V: MainViewReceiver>(view: &V, combined_patches: &Vec<CombinedPatch>) -> Result<(), failure::Error> {
			view.show_combined_patches(combined_patches.iter().map(|patch| patch.info.clone()).collect())
		}

		match command {
			Command::GetBranches => {
				MainModel::get_branches_and_commits(view, state)?;
			}
			Command::ImportCommits(commits) => {
				let mut new_combined_patches = Vec::<CombinedPatch>::new();
				for commit in commits {
					let combined_patch_data = state.git.diff_tree(&commit.hash)?;
					let patches = parse_combined_patch(combined_patch_data.as_bytes())?;
					let combined_patch = CombinedPatch {
						info: commit.info.change_set_info,
						patches,
					};
					new_combined_patches.push(combined_patch);
				}

				state.combined_patches.extend(new_combined_patches);
				show_combined_patches(view, &state.combined_patches)?;
			}
			Command::SetPatchMessage(patch_index, message) => {
				state.combined_patches[patch_index].info.message = message;
				show_combined_patches(view, &state.combined_patches)?;
			}
			Command::MovePatch(source_position, insertion_position) => {
				if source_position != insertion_position {
					let mut adjusted_insertion_position = insertion_position;
					if source_position < insertion_position {
						adjusted_insertion_position -= 1;
					}

					let patch = state.combined_patches.remove(source_position);
					state.combined_patches.insert(adjusted_insertion_position, patch);
					show_combined_patches(view, &state.combined_patches)?;
				}
			}
			Command::DeletePatch(patch_index) => {
				state.combined_patches.remove(patch_index);
				show_combined_patches(view, &state.combined_patches)?;
			}
			Command::ApplyCommits(first_commit_to_replace) => {
				let active_branch = state.git.symbolic_ref("HEAD")?;
				// TODO: this is the scenario when we are in a clean state, HEAD points to the changed branch. Consider other states.
				let mut target_commit = first_commit_to_replace.info.parent;
				let mut applied_patches = 0usize;
				for patch in state.combined_patches.iter().rev() {
					state.git.read_tree(target_commit.clone())?;

					let mut patch_data: Vec<u8> = Vec::new();
					patch.write(&mut patch_data)?;

					let result = state.git.apply(&*patch_data, false);
					match result {
						Err(ref err) if err.to_status() == Some(1) => {
							state.git.checkout_index()?; // TODO: can the index have a half-applied patch at this point?

							let result = state.git.apply(&*patch_data, true);
							match result {
								Err(ref err) if err.to_status() == Some(1) => (),
								Err(err) => return Err(err.into()),
								Ok(()) => return Err(MainModelError::ApplyPatchesError(String::from("Expected to have conflicts, but none found"),
									Backtrace::new()).into())
							}

							let conflicts = state.git.status_conflicts()?;
							// TODO: show conflicts, remove applied patches
							for file in conflicts {
								println!("{}", file);
							}
							return Ok(()); // TODO: then do git.update_index() for these files
						}
						Err(err) => return Err(err.into()),
						Ok(_) => ()
					};

					let tree = state.git.write_tree()?;
					let commit = state.git.commit_tree(&tree, target_commit.as_ref(), &patch.info.message)?;
					state.git.update_ref("HEAD", &commit)?;

					target_commit = Some(commit);
					applied_patches += 1;
				}

				let patches_left = state.combined_patches.len() - applied_patches;
				state.combined_patches.truncate(patches_left);

				if patches_left == 0 {
					state.git.update_ref(&active_branch, &target_commit.unwrap())?;
					state.git.symbolic_ref_update("HEAD", &active_branch)?;
				}

				MainModel::get_branches_and_commits(view, state)?;
			}
			Command::SwitchToBranch(ref_name) => {
				state.git.symbolic_ref_update("HEAD", &ref_name)?;
				MainModel::get_branches_and_commits(view, state)?;
			}
		}
		Ok(())
	}

	fn get_branches_and_commits(view: &MainViewReceiver, State { ref mut git, ref mut combined_patches }: &mut State) -> Result<(), failure::Error> {
		let refs = git.show_refs_heads()?;
		let unprocessed_parts_to_refs = refs
				.iter()
				.filter(|ref_name| ref_name.starts_with(git::BRANCH_PREFIX))
				.map(|ref_name| (&ref_name[git::BRANCH_PREFIX.len()..], ref_name.as_str()))
				.collect();

		let head_target = git.symbolic_ref("HEAD")?;
		let active_branch = if head_target.starts_with(git::BRANCH_PREFIX) { Some(head_target.as_str()) } else { None };

		view.show_branches(BranchItem::from_refs(unprocessed_parts_to_refs, &active_branch))?;

		let merges = git.rev_list(None, true)?;
		let commit_hashes = git.rev_list(if merges.is_empty() { None } else { Some(&merges[0]) }, false)?;

		let mut commits = Vec::<Commit>::new();
		for hash in commit_hashes {
			let commit_info_str = git.cat_file(&hash)?;
			let commit = Commit::from_data(hash, commit_info_str.as_bytes())?;
			commits.push(commit);
		}
		view.show_commits(commits)?;
		view.show_combined_patches(combined_patches.iter().map(|patch| patch.info.clone()).collect())
	}

	pub fn import_commits(&self, commits: Vec<Commit>) {
		self.base.send(Command::ImportCommits(commits));
	}

	pub fn set_patch_message(&self, patch_index: usize, message: String) {
		self.base.send(Command::SetPatchMessage(patch_index, message));
	}

	pub fn move_patch(&self, source_position: usize, insertion_position: usize) {
		self.base.send(Command::MovePatch(source_position, insertion_position));
	}

	pub fn delete(&self, patch_index: usize) {
		self.base.send(Command::DeletePatch(patch_index));
	}

	pub fn apply_patches(&self, first_commit_to_replace: Commit) {
		self.base.send(Command::ApplyCommits(first_commit_to_replace));
	}

	pub fn switch_to_branch(&self, ref_name: &str) {
		self.base.send(Command::SwitchToBranch(String::from(ref_name)));
	}
}

#[derive(Fail, Debug)]
enum MainModelError {
	#[fail(display = "Error when applying patches: {}", _0)]
	ApplyPatchesError(String, Backtrace),
}

pub trait MainViewReceiver: View {
	fn show_branches(&self, branches: Vec<BranchItem>) -> Result<(), failure::Error>;
	fn show_commits(&self, commits: Vec<Commit>) -> Result<(), failure::Error>;
	fn show_combined_patches(&self, combined_patches: Vec<ChangeSetInfo>) -> Result<(), failure::Error>;
}

pub enum BranchItem {
	Folder {
		display_name: String,
		children: Vec<BranchItem>,
		has_active_child: bool,
	},
	Branch {
		ref_name: String,
		display_name: String,
		active: bool,
	},
}

impl BranchItem {
	fn from_refs(unprocessed_parts_to_refs_map: Vec<(&str, &str)>, active_branch: &Option<&str>) -> Vec<BranchItem> {
		let mut folders: HashMap<&str, (Vec<(&str, &str)>, bool)> = HashMap::new();
		let mut branches = Vec::new();
		for (parts, ref_name) in unprocessed_parts_to_refs_map {
			let first_slash_pos = parts.find("/");
			let active = active_branch.map(|active_branch_name| active_branch_name == ref_name).unwrap_or(false);
			match first_slash_pos {
				Some(pos) => {
					let (folder_name, rest) = parts.split_at(pos);
					let empty = match folders.get_mut(folder_name) {
						None => {
							true
						}
						Some(&mut (ref mut sub_items, ref mut has_active_child)) => {
							sub_items.push((&rest[1..], ref_name));
							if active {
								*has_active_child = active;
							}
							false
						}
					};

					if empty { // No non-lexical lifetimes yet!
						let sub_items = vec![(&rest[1..], ref_name)];
						folders.insert(folder_name, (sub_items, active));
					}
				}
				None => {
					let branch = BranchItem::Branch {
						ref_name: ref_name.into(),
						display_name: parts.into(),
						active,
					};
					branches.push(branch);
				}
			}
		}

		let mut branch_items: Vec<BranchItem> = folders
				.into_iter()
				.map(|(folder_name, (sub_items, has_active_child))| {
					let children = BranchItem::from_refs(sub_items, active_branch);
					BranchItem::Folder {
						display_name: folder_name.into(),
						children,
						has_active_child,
					}
				})
				.collect();
		branch_items.extend(branches.into_iter());
		branch_items
	}
}