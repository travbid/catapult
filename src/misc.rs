use std::path::{Path, PathBuf};

pub(crate) fn canonicalize(parent_path: &Path, x: &String) -> String {
	let path = PathBuf::from(x);
	if path.is_absolute() {
		x.to_owned()
	} else {
		parent_path.join(x).canonicalize().unwrap().to_str().unwrap().to_owned()
	}
}
