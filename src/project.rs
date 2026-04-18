use std::{
	path::PathBuf, //
	sync::Arc,
};

use crate::{
	executable::Executable, //
	interface_library::InterfaceLibrary,
	link_type::LinkPtr,
	object_library::ObjectLibrary,
	shared_library::SharedLibrary,
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
	pub link_targets: Vec<LinkPtr>,
	pub static_libraries: Vec<Arc<StaticLibrary>>,
	pub object_libraries: Vec<Arc<ObjectLibrary>>,
	pub interface_libraries: Vec<Arc<InterfaceLibrary>>,
	pub shared_libraries: Vec<Arc<SharedLibrary>>,
}
