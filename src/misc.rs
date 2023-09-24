use std::path::{Path, PathBuf};

pub(crate) fn canonicalize(parent_path: &Path, x: &String) -> PathBuf {
	let path = PathBuf::from(x);
	if path.is_absolute() {
		path
	} else {
		let joined = parent_path.join(x);
		match joined.canonicalize() {
			Ok(path) => path,
			Err(e) => {
				log::warn!("Could not canonicalize path \"{}\": {}", joined.to_string_lossy(), e);
				joined
			}
		}
		// TODO(Travers): Check if there's a way to make clang/gcc/msvc support UNC paths
		// Implement dunce::canonicalize() ?
	}
}

pub(crate) fn is_c_source(src_filename: &&String) -> bool {
	src_filename.ends_with(".c") || src_filename.ends_with(".C")
}

pub(crate) fn is_cpp_source(src_filename: &&String) -> bool {
	!is_c_source(src_filename)
}
