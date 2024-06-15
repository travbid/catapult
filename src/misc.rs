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

pub(crate) fn is_c_source(src_filename: &str) -> bool {
	src_filename.ends_with(".c") || src_filename.ends_with(".C")
}

pub(crate) fn is_cpp_source(src_filename: &str) -> bool {
	src_filename.ends_with(".cpp") || src_filename.ends_with(".cc")
}

#[derive(Debug, Default)]
pub struct Sources {
	pub c: Vec<SourcePath>,
	pub cpp: Vec<SourcePath>,
}

impl Sources {
	pub fn iter(&self) -> impl Iterator<Item = &SourcePath> {
		self.c.iter().chain(self.cpp.iter())
	}

	pub(crate) fn from_slice(sources: &[String], parent_path: &Path) -> Result<Self, String> {
		sources
			.iter()
			.map(|x| join_parent(parent_path, x))
			.try_fold(Sources::default(), |mut acc, src| {
				if is_c_source(&src.name) {
					acc.c.push(src);
				} else if is_cpp_source(&src.name) {
					acc.cpp.push(src);
				} else {
					return Err(format!("Unknown source type: {}", &src.name));
				}
				Ok(acc)
			})
	}
}
