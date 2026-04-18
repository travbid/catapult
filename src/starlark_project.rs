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
	starlark_module, //
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
	link_type::LinkPtr,
	project::{Project, ProjectInfo},
	starlark_executable::StarExecutable, //
	starlark_interface_library::StarIfaceLibWrapper,
	starlark_link_target::{PtrLinkTarget, StarLinkTargetRef},
	starlark_object_library::StarObjLibWrapper,
	starlark_shared_library::StarSharedLibWrapper,
	starlark_static_library::StarStaticLibWrapper,
	target::Target,
};

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarProject {
	pub name: String,
	pub path: PathBuf,
	pub dependencies: Vec<Arc<StarProject>>,
	pub executables: Vec<Arc<StarExecutable>>,
	pub link_targets: Vec<StarLinkTargetRef>,
	pub generator_names: HashMap<String, OwnedFrozenValue>,
}

impl fmt::Display for StarProject {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, r#"Project {{}}"#)
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
		for link_target in &self.link_targets {
			if link_target.name() == attribute {
				let value = match link_target {
					StarLinkTargetRef::Static(lib) => heap.alloc(StarStaticLibWrapper(lib.clone())),
					StarLinkTargetRef::Object(lib) => heap.alloc(StarObjLibWrapper(lib.clone())),
					StarLinkTargetRef::Interface(lib) => heap.alloc(StarIfaceLibWrapper(lib.clone())),
					StarLinkTargetRef::Shared(lib) => heap.alloc(StarSharedLibWrapper(lib.clone())),
				};
				return Some(value);
			}
		}
		None
	}
	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		self.link_targets.iter().any(|x| x.name() == attribute)
	}

	fn dir_attr(&self) -> Vec<String> {
		self.link_targets.iter().map(|x| x.name().to_owned()).collect()
	}
}

starlark_simple_value!(StarProject);

pub(super) struct StarLinkTargetCache(HashMap<PtrLinkTarget, LinkPtr>);

impl StarLinkTargetCache {
	fn new() -> StarLinkTargetCache {
		StarLinkTargetCache(HashMap::new())
	}
	pub fn get(&self, key: &PtrLinkTarget) -> Option<LinkPtr> {
		self.0.get(key).cloned()
	}
	pub fn insert(&mut self, key: PtrLinkTarget, value: LinkPtr) {
		self.0.insert(key, value);
	}
}

impl StarProject {
	pub fn new(name: String, path: PathBuf, dependencies: Vec<Arc<StarProject>>) -> Self {
		StarProject {
			name,
			path,
			dependencies,
			executables: Vec::new(),
			link_targets: Vec::new(),
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
			link_targets: self
			.link_targets
			.iter()
			.map(|x| -> Result<LinkPtr,String> {
				let ptr = x.as_ptr_link_target();
				if let Some(lib) = link_map.get(&ptr) {
					Ok(lib)
				} else {
					let data = ptr.0.as_link_target(Weak::new(), &self.path, ptr.clone(), link_map, &self.generator_names)?;
						// let arc = Arc::new(data);
						link_map.insert(ptr, data.clone());
						Ok(data)
				}
			})
			.collect::<Result<_, _>>()?,
		};

		validate_library_dependency_order(&project)?;

		let ret = Arc::<Project>::new_cyclic(move |weak_parent: &Weak<Project>| -> Project {
			// We need one of the following to set the Weak parent without using unsafe:
			// - https://github.com/rust-lang/libs-team/issues/90
			// - https://github.com/rust-lang/rust/issues/112566
			for exe in &mut project.executables {
				Arc::get_mut(exe).unwrap().set_parent(weak_parent.clone());
			}

			for lib in &mut project.link_targets {
				match lib {
					LinkPtr::Static(lib) => {
						let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
						lib_mut.set_parent(weak_parent.clone());
					}
					LinkPtr::Object(lib) => {
						let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
						lib_mut.set_parent(weak_parent.clone());
					}
					LinkPtr::Interface(lib) => {
						let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
						lib_mut.set_parent(weak_parent.clone());
					}
					LinkPtr::Shared(lib) => {
						let lib_mut = unsafe { &mut (*Arc::as_ptr(lib).cast_mut()) };
						lib_mut.set_parent(weak_parent.clone());
					}
				}
			}
			project
		});

		Ok(ret)
	}
}

fn validate_library_dependency_order(project: &Project) -> Result<(), String> {
	let mut all_targets = HashSet::new();
	let mut seen_targets = HashSet::new();
	for target in &project.link_targets {
		all_targets.insert(target.clone());
	}
	for target in &project.link_targets {
		for dependency in target.internal_links() {
			if all_targets.contains(&dependency) && !seen_targets.contains(&dependency) {
				return Err(format!(
					"Library order violation in project \"{}\": target \"{}\" depends on \"{}\" which is declared later. Declare \"{}\" before \"{}\".",
					project.info.name,
					target.name(),
					dependency.name(),
					dependency.name(),
					target.name()
				));
			}
		}
		seen_targets.insert(target.clone());
	}
	Ok(())
}
