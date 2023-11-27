use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct SourcePath {
	pub full: PathBuf,
	pub name: String,
}

pub(crate) fn join_parent(parent_path: &Path, x: &String) -> SourcePath {
	let joined = parent_path.join(x); // If x is absolute, it replaces the current path.
	match joined.try_exists() {
		Ok(true) => match joined.canonicalize() {
			Ok(path) => SourcePath { full: path, name: x.clone() },
			Err(e) => {
				log::warn!("Could not canonicalize path \"{}\": {}", joined.to_string_lossy(), e);
				SourcePath { full: joined, name: x.clone() }
			}
		},
		Ok(false) => {
			log::warn!("Path does not exist: \"{}\"", joined.to_string_lossy());
			SourcePath { full: joined, name: x.clone() }
		}
		Err(e) => {
			log::warn!("Existence of path could not be confirmed \"{}\": {}", joined.to_string_lossy(), e);
			SourcePath { full: joined, name: x.clone() }
		}
	}
	// TODO(Travers): Check if there's a way to make clang/gcc/msvc support UNC paths
	// Implement dunce::canonicalize() ?
}

pub(crate) fn is_c_source(src_filename: &&String) -> bool {
	src_filename.ends_with(".c") || src_filename.ends_with(".C")
}

pub(crate) fn is_cpp_source(src_filename: &&String) -> bool {
	!is_c_source(src_filename)
}
