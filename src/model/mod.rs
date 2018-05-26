use std::sync;
use std::thread;

use failure;
use std::sync::Arc;

pub mod main;

#[derive(Clone)]
struct Model<C: Send + 'static> {
	worker_sink: sync::mpsc::Sender<C>
}

impl<C: Send> Model<C> {
	fn new<V: View + ?Sized, P: Send + 'static, S: 'static>(view: Arc<V>, params: P, state_initializer: fn(P) -> Result<S, failure::Error>,
		command_receiver: fn(&V, &mut S, C) -> Result<(), failure::Error>) -> Model<C> {
		let (sender, receiver) = sync::mpsc::channel();

		let model = Model {
			worker_sink: sender
		};

		let view_ref = view.clone();
		thread::spawn(move || {
			let state_result = state_initializer(params);
			let mut state = match state_result {
				Ok(result) => result,
				Err(err) => {
					view.error(err);
					return;
				}
			};

			loop {
				let command = match receiver.recv() {
					Ok(command) => command,
					Err(_) => break
				};
				let result = command_receiver(&*view_ref, &mut state, command);
				if let Err(error) = result {
					view_ref.error(error);
				}
			}
		});

		model
	}

	fn send(&self, command: C) {
		self.worker_sink.send(command).unwrap();
	}
}

pub trait View: Send + Sync + 'static {
	fn error(&self, error: failure::Error);
}