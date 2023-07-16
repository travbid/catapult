use std::{
	path::PathBuf, //
	sync::Arc,
};

use crate::{
	executable::Executable, //
	library::Library,
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
	pub libraries: Vec<Arc<Library>>,
}

impl Project {
}

