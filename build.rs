extern crate embed_resource;

fn main() {
	if cfg!(windows) {
		embed_resource::compile("resources/windows/resources.rc");
	}
}