use std::{
	path::PathBuf, //
	sync::Arc,
};

use crate::{
	executable::Executable, //
	static_library::StaticLibrary,
};

#[derive(Debug)]
pub struct ProjectInfo {
	pub name: String,
	pub path: PathBuf,
}

#[derive(Debug)]
pub struct Project {
	pub info: Arc<ProjectInfo>,
    pub dependencies: Vec<Arc<Project>>,
	pub executables: Vec<Arc<Executable>>,
	pub static_libraries: Vec<Arc<StaticLibrary>>,
}

impl Project {}
