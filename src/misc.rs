use std::{
	io,
	path::{Path, PathBuf},
};

pub(crate) fn canonicalize(parent_path: &Path, x: &String) -> io::Result<String> {
	let path = PathBuf::from(x);
	if path.is_absolute() {
		Ok(x.to_owned())
	} else {
		let canon = parent_path.join(x).canonicalize()?;
		// TODO(Travers): Check if there's a way to make clang/gcc/msvc support UNC paths
		let str = canon.to_str().unwrap().trim_start_matches(r"\\?\");
		Ok(str.to_owned())
	}
}

pub(crate) fn is_c_source(src_filename: &&String) -> bool {
	src_filename.ends_with(".c") || src_filename.ends_with(".C")
}

pub(crate) fn is_cpp_source(src_filename: &&String) -> bool {
	!is_c_source(src_filename)
}
