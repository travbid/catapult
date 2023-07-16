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
	library::Library, //
	project::Project,
	starlark_link_target::{PtrLinkTarget, StarLinkTarget},
	starlark_project::{StarLinkTargetCache, StarProject},
	target::LinkTarget,
};

#[derive(Clone, Debug, ProvidesStaticType, Allocative)]
pub(super) struct StarLibrary {
	pub parent_project: Weak<Mutex<StarProject>>,
	pub name: String,
	pub sources: Vec<String>,
	pub private_links: Vec<Arc<dyn StarLinkTarget>>,
	pub include_dirs_public: Vec<String>,
	pub include_dirs_private: Vec<String>,
	pub output_name: Option<String>,
}

impl fmt::Display for StarLibrary {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let mut sources = self.sources.join(",\n      ");
		if self.sources.len() > 1 {
			sources = String::from("\n      ") + &sources + ",\n   "
		}
		write!(
			f,
			r#"StarLibrary{{
   name: {},
   sources: [{:?}],
}}"#,
			self.name, sources
		)
	}
}


impl StarLinkTarget for StarLibrary {
	fn as_link_target(
		&self,
		parent: Weak<Project>,
		ptr: PtrLinkTarget,
		link_map: &mut StarLinkTargetCache,
	) -> Arc<dyn LinkTarget> {
		let arc = Arc::new(self.as_library(parent, link_map));
		// let ptr = PtrLinkTarget(arc.clone());
		link_map.insert_library(ptr, arc.clone());
		arc
	}
}

impl StarLibrary {
	fn public_includes_recursive(&self) -> Vec<String> {
		let public_includes = self.include_dirs_public.clone();
		// for link in &self.private_links {parent_map: &HashMap<PtrLinkTarget, Weak<Project>>
		//     public_includes.extend(link.public_includes_recursive());
		// }
		public_includes
	}
	pub fn as_library(&self, parent_project: Weak<Project>, link_map: &mut StarLinkTargetCache) -> Library {
		Library {
			parent_project: parent_project.clone(),
			name: self.name.clone(),
			sources: self.sources.clone(),
			include_dirs_private: self.include_dirs_private.clone(),
			include_dirs_public: self.include_dirs_public.clone(),
			private_links: self
				.private_links
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
			output_name: self.output_name.clone(),
		}
	}
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarLibraryWrapper(pub(super) Arc<StarLibrary>);

impl fmt::Display for StarLibraryWrapper {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

// #[starlark_value(type = "Library")] //, UnpackValue, StarlarkTypeRepr)]
impl<'v> StarlarkValue<'v> for StarLibraryWrapper {
	starlark_type!("Library");
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

starlark_simple_value!(StarLibraryWrapper);

#[starlark_module]
fn library_methods_impl(builder: &mut MethodsBuilder) {
	fn name<'v>(
		this: &'v StarLibraryWrapper,
		heap: &'v Heap,
	) -> anyhow::Result<StringValue<'v>> {
		Ok(heap.alloc_str(&format!(":{}", this.0.name)))
	}
}

fn library_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(library_methods_impl)
}
