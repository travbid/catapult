use core::fmt;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	sync::Arc,
};

use allocative::Allocative;
use starlark::{
	environment::{
		Methods, //
		MethodsBuilder,
		MethodsStatic,
	},
	// starlark_complex_value,
	starlark_module,
	starlark_simple_value,
	starlark_type,
	values::{
		Heap, //
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

use crate::{
	interface_library::InterfaceLibrary,
	link_type::LinkPtr,
	project::{Project, ProjectInfo},
	starlark_executable::StarExecutable, //
	starlark_interface_library::{StarIfaceLibrary, StarIfaceLibraryWrapper},
	starlark_library::{StarLibrary, StarLibraryWrapper},
	starlark_link_target::PtrLinkTarget,
	static_library::StaticLibrary,
};

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarProject {
	pub name: String,
	pub path: PathBuf,
	pub dependencies: Vec<Arc<StarProject>>,
	pub executables: Vec<Arc<StarExecutable>>,
	pub libraries: Vec<Arc<StarLibrary>>,
	pub interface_libraries: Vec<Arc<StarIfaceLibrary>>,
}

impl fmt::Display for StarProject {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, r#"Project{{}}"#)
	}
}

#[starlark_module]
fn project_methods_impl(builder: &mut MethodsBuilder) {}

fn project_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(project_methods_impl)
}

impl<'v> StarlarkValue<'v> for StarProject {
	starlark_type!("Project");
	fn get_methods() -> Option<&'static Methods> {
		project_methods()
	}
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		for lib in &self.libraries {
			if lib.name == attribute {
				return Some(heap.alloc(StarLibraryWrapper(lib.clone())));
			}
		}
		for lib in &self.interface_libraries {
			if lib.name == attribute {
				return Some(heap.alloc(StarIfaceLibraryWrapper(lib.clone())));
			}
		}
		None
	}
	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		for lib in &self.libraries {
			if lib.name == attribute {
				return true;
			}
		}
		for lib in &self.interface_libraries {
			if lib.name == attribute {
				return true;
			}
		}
		false
	}

	fn dir_attr(&self) -> Vec<String> {
		let mut attrs = Vec::new();
		for lib in &self.libraries {
			attrs.push(lib.name.to_owned());
		}
		for lib in &self.interface_libraries {
			attrs.push(lib.name.to_owned());
		}
		attrs
	}
}

starlark_simple_value!(StarProject);

pub(super) struct StarLinkTargetCache {
	all_targets: HashSet<PtrLinkTarget>,
	static_libs: HashMap<PtrLinkTarget, Arc<StaticLibrary>>,
	interface_libs: HashMap<PtrLinkTarget, Arc<InterfaceLibrary>>,
}

impl StarLinkTargetCache {
	fn new() -> StarLinkTargetCache {
		StarLinkTargetCache {
			all_targets: HashSet::new(),
			static_libs: HashMap::new(),
			interface_libs: HashMap::new(),
		}
	}
	pub fn get_static(&self, key: &PtrLinkTarget) -> Option<&Arc<StaticLibrary>> {
		if self.all_targets.contains(key) {
			self.static_libs.get(key)
		} else {
			None
		}
	}
	pub fn get_interface(&self, key: &PtrLinkTarget) -> Option<&Arc<InterfaceLibrary>> {
		if self.all_targets.contains(key) {
			self.interface_libs.get(key)
		} else {
			None
		}
	}
	pub fn get(&self, key: &PtrLinkTarget) -> Option<LinkPtr> {
		match self.get_static(key) {
			Some(x) => return Some(LinkPtr::Static(x.clone())),
			None => (),
		}
		match self.get_interface(key) {
			Some(x) => return Some(LinkPtr::Interface(x.clone())),
			None => (),
		}
		None
	}
	pub fn insert_static(&mut self, key: PtrLinkTarget, value: Arc<StaticLibrary>) {
		self.static_libs.insert(key.clone(), value);
		self.all_targets.insert(key);
	}
	pub fn insert_interface(&mut self, key: PtrLinkTarget, value: Arc<InterfaceLibrary>) {
		self.interface_libs.insert(key.clone(), value);
		self.all_targets.insert(key);
	}
}

impl StarProject {
	pub fn new(name: String, path: PathBuf, dependencies: Vec<Arc<StarProject>>) -> Self {
		StarProject {
			name,
			path,
			dependencies,
			executables: Vec::new(),
			libraries: Vec::new(),
			interface_libraries: Vec::new(),
		}
	}

	pub fn into_project(self) -> Arc<Project> {
		let mut cache = StarLinkTargetCache::new();
		self.as_project_inner(&mut cache)
	}
	fn as_project_inner(&self, link_map: &mut StarLinkTargetCache) -> Arc<Project> {
		let project = Arc::<Project>::new_cyclic(|weak_parent| {
			Project {
				info: Arc::new(ProjectInfo { name: self.name.clone(), path: self.path.clone() }),
				dependencies: self.dependencies.iter().map(|x| x.as_project_inner(link_map)).collect(),
				executables: self
					.executables
					.iter()
					.map(|x| Arc::new(x.as_executable(weak_parent.clone(), link_map)))
					.collect(),
				static_libraries: self
					.libraries
					.iter()
					.map(|x| {
						let ptr = PtrLinkTarget(x.clone());
						if let Some(lib) = link_map.get_static(&ptr) {
							lib.clone()
						} else {
							let arc = Arc::new(x.as_library(weak_parent.clone(), link_map));
							link_map.insert_static(ptr, arc.clone());
							arc
						}
					})
					.collect(),
				interface_libraries: self
					.interface_libraries
					.iter()
					.map(|x| {
						let ptr = PtrLinkTarget(x.clone());
						if let Some(lib) = link_map.get_interface(&ptr) {
							lib.clone()
						} else {
							let arc = Arc::new(x.as_library(weak_parent.clone(), link_map));
							link_map.insert_interface(ptr, arc.clone());
							arc
						}
					})
					.collect(),
			}
		});

		project
	}
}
