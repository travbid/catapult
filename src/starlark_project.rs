use core::fmt;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	sync::{Arc, Weak},
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
	values::{
		Heap, //
		NoSerialize,
		OwnedFrozenValue,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

use crate::{
	interface_library::InterfaceLibrary,
	link_type::LinkPtr,
	object_library::ObjectLibrary,
	project::{Project, ProjectInfo},
	starlark_executable::StarExecutable, //
	starlark_interface_library::{StarIfaceLibrary, StarIfaceLibraryWrapper},
	starlark_link_target::PtrLinkTarget,
	starlark_object_library::{StarObjLibWrapper, StarObjectLibrary},
	starlark_static_library::{StarStaticLibWrapper, StarStaticLibrary},
	static_library::StaticLibrary,
};

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarProject {
	pub name: String,
	pub path: PathBuf,
	pub dependencies: Vec<Arc<StarProject>>,
	pub executables: Vec<Arc<StarExecutable>>,
	pub static_libraries: Vec<Arc<StarStaticLibrary>>,
	pub object_libraries: Vec<Arc<StarObjectLibrary>>,
	pub interface_libraries: Vec<Arc<StarIfaceLibrary>>,

	pub generator_names: HashMap<String, OwnedFrozenValue>,
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

#[starlark::values::starlark_value(type = "Project")]
impl<'v> StarlarkValue<'v> for StarProject {
	fn get_methods() -> Option<&'static Methods> {
		project_methods()
	}
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		for lib in &self.static_libraries {
			if lib.name == attribute {
				return Some(heap.alloc(StarStaticLibWrapper(lib.clone())));
			}
		}
		for lib in &self.object_libraries {
			if lib.name == attribute {
				return Some(heap.alloc(StarObjLibWrapper(lib.clone())));
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
		for lib in &self.static_libraries {
			if lib.name == attribute {
				return true;
			}
		}
		for lib in &self.object_libraries {
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
		for lib in &self.static_libraries {
			attrs.push(lib.name.to_owned());
		}
		for lib in &self.object_libraries {
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
	object_libs: HashMap<PtrLinkTarget, Arc<ObjectLibrary>>,
	interface_libs: HashMap<PtrLinkTarget, Arc<InterfaceLibrary>>,
}

impl StarLinkTargetCache {
	fn new() -> StarLinkTargetCache {
		StarLinkTargetCache {
			all_targets: HashSet::new(),
			static_libs: HashMap::new(),
			object_libs: HashMap::new(),
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
	pub fn get_object(&self, key: &PtrLinkTarget) -> Option<&Arc<ObjectLibrary>> {
		if self.all_targets.contains(key) {
			self.object_libs.get(key)
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
		if let Some(x) = self.get_static(key) {
			return Some(LinkPtr::Static(x.clone()));
		}
		if let Some(x) = self.get_object(key) {
			return Some(LinkPtr::Object(x.clone()));
		}
		if let Some(x) = self.get_interface(key) {
			return Some(LinkPtr::Interface(x.clone()));
		}
		None
	}
	pub fn insert_static(&mut self, key: PtrLinkTarget, value: Arc<StaticLibrary>) {
		self.static_libs.insert(key.clone(), value);
		self.all_targets.insert(key);
	}
	pub fn insert_object(&mut self, key: PtrLinkTarget, value: Arc<ObjectLibrary>) {
		self.object_libs.insert(key.clone(), value);
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
			static_libraries: Vec::new(),
			object_libraries: Vec::new(),
			interface_libraries: Vec::new(),

			generator_names: HashMap::new(),
		}
	}

	pub fn into_project(self) -> Result<Arc<Project>, String> {
		let mut cache = StarLinkTargetCache::new();
		self.as_project_inner(&mut cache)
	}

	fn as_project_inner(&self, link_map: &mut StarLinkTargetCache) -> Result<Arc<Project>, String> {
		let mut project = //Arc::<Project>::new_cyclic(|weak_parent| 
		Project {
			info: Arc::new(ProjectInfo { name: self.name.clone(), path: self.path.clone() }),
			dependencies: self.dependencies.iter().map(|x| x.as_project_inner(link_map)).collect::<Result<_,_>>()?,
			executables: self
				.executables
				.iter()
				.map(|x| -> Result<Arc<_>,String> {
					let data = x.as_executable(Weak::new(), &self.path, link_map, &self.generator_names)?;
					Ok(Arc::new(
						data
					))
				}
			)
				.collect::<Result<_,_>>()?,
			static_libraries: self
				.static_libraries
				.iter()
				.map(|x| -> Result<Arc<_>,String>{
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lib) = link_map.get_static(&ptr) {
						Ok(lib.clone())
					} else {
						let data = x.as_library(Weak::new(), &self.path, link_map, &self.generator_names)?;
						let arc = Arc::new(data);
						link_map.insert_static(ptr, arc.clone());
						Ok(arc)
					}
				})
				.collect::<Result<_,_>>()?,
			object_libraries: self
				.object_libraries
				.iter()
				.map(|x| -> Result<Arc<_>,String>{
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lib) = link_map.get_object(&ptr) {
						Ok(lib.clone())
					} else {
						let data = x.as_library(Weak::new(), &self.path, link_map, &self.generator_names)?;
						let arc = Arc::new(data);
						link_map.insert_object(ptr, arc.clone());
						Ok(arc)
					}
				})
				.collect::<Result<_,_>>()?,
			interface_libraries: self
				.interface_libraries
				.iter()
				.map(|x| -> Result<_,String>{
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lib) = link_map.get_interface(&ptr) {
						Ok(lib.clone())
					} else {
						let data = x.as_library(Weak::new(), &self.path, link_map, &self.generator_names)?;
						let arc = Arc::new(data);
						link_map.insert_interface(ptr, arc.clone());
						Ok(arc)
					}
				})
				.collect::<Result<_,_>>()?,
		}; //);

		let ret = Arc::<Project>::new_cyclic(move |weak_parent: &Weak<Project>| -> Project {
			// We need one of the following to set the Weak parent without using unsafe:
			// - https://github.com/rust-lang/libs-team/issues/90
			// - https://github.com/rust-lang/rust/issues/112566
			for exe in &mut project.executables {
				Arc::get_mut(exe).unwrap().set_parent(weak_parent.clone());
			}
			for lib in &mut project.static_libraries {
				let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
				lib_mut.set_parent(weak_parent.clone());
			}
			for lib in &mut project.object_libraries {
				let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
				lib_mut.set_parent(weak_parent.clone());
			}
			for lib in &mut project.interface_libraries {
				let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
				lib_mut.set_parent(weak_parent.clone());
			}
			project
		});

		Ok(ret)
	}
}
