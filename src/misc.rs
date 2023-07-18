use std::path::{Path, PathBuf};

pub(crate) fn canonicalize(parent_path: &Path, x: &String) -> String {
	let path = PathBuf::from(x);
	if path.is_absolute() {
		x.to_owned()
	} else {
		parent_path.join(x).canonicalize().unwrap().to_str().unwrap().to_owned()
	}
}

pub(crate) fn is_c_source(src_filename: &&String) -> bool {
	src_filename.ends_with(".c") || src_filename.ends_with(".C")
}

pub(crate) fn is_cpp_source(src_filename: &&String) -> bool {
	!is_c_source(src_filename)
}
