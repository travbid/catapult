use core::fmt;
use std::sync::{Arc, Mutex, Weak};

use allocative::Allocative;
use starlark::{
	environment::{
		Methods, //
		MethodsBuilder,
		MethodsStatic,
	},
	starlark_module, //
	starlark_simple_value,
	starlark_type,
	values::{
		Heap, //
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		StringValue,
		Value,
	},
};

use super::{
	interface_library::InterfaceLibrary, //
	link_type::LinkPtr,
	project::Project,
	starlark_link_target::{PtrLinkTarget, StarLinkTarget},
	starlark_project::{StarLinkTargetCache, StarProject},
};

#[derive(Clone, Debug, ProvidesStaticType, Allocative)]
pub(super) struct StarIfaceLibrary {
	pub parent_project: Weak<Mutex<StarProject>>,
	pub name: String,
	pub links: Vec<Arc<dyn StarLinkTarget>>,
	pub include_dirs: Vec<String>,
	pub defines: Vec<String>,
	pub link_flags: Vec<String>,
}

impl fmt::Display for StarIfaceLibrary {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"StarIfaceLibrary{{
   name: {},
}}"#,
			self.name
		)
	}
}

impl StarLinkTarget for StarIfaceLibrary {
	fn as_link_target(&self, parent: Weak<Project>, ptr: PtrLinkTarget, link_map: &mut StarLinkTargetCache) -> LinkPtr {
		let arc = Arc::new(self.as_library(parent, link_map));
		// let ptr = PtrLinkTarget(arc.clone());
		link_map.insert_interface(ptr, arc.clone());
		LinkPtr::Interface(arc)
	}
	fn public_includes_recursive(&self) -> Vec<String> {
		let mut public_includes = self.include_dirs.clone();
		for link in &self.links {
			public_includes.extend(link.public_includes_recursive());
		}
		public_includes
	}
}

impl StarIfaceLibrary {
	pub fn as_library(&self, parent_project: Weak<Project>, link_map: &mut StarLinkTargetCache) -> InterfaceLibrary {
		InterfaceLibrary {
			parent_project: parent_project.clone(),
			name: self.name.clone(),
			include_dirs: self.include_dirs.clone(),
			links: self
				.links
				.iter()
				.map(|x| {
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lt) = link_map.get(&ptr) {
						lt
					} else {
						x.as_link_target(parent_project.clone(), ptr, link_map)
					}
				})
				.collect(),
			defines: self.defines.clone(),
			link_flags: self.link_flags.clone(),
		}
	}
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarIfaceLibraryWrapper(pub(super) Arc<StarIfaceLibrary>);

impl fmt::Display for StarIfaceLibraryWrapper {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

// #[starlark_value(type = "Library")] //, UnpackValue, StarlarkTypeRepr)]
impl<'v> StarlarkValue<'v> for StarIfaceLibraryWrapper {
	starlark_type!("InterfaceLibrary");
	fn get_methods() -> Option<&'static Methods> {
		library_methods()
	}
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"include_dirs" => Some(heap.alloc(self.0.public_includes_recursive())),
			_ => None,
		}
	}
	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		attribute == "include_dirs"
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["include_dirs".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarIfaceLibraryWrapper);

#[starlark_module]
fn library_methods_impl(builder: &mut MethodsBuilder) {
	fn name<'v>(this: &'v StarIfaceLibraryWrapper, heap: &'v Heap) -> anyhow::Result<StringValue<'v>> {
		Ok(heap.alloc_str(&format!(":{}", this.0.name)))
	}
}

fn library_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(library_methods_impl)
}
