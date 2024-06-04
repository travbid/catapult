use core::fmt;
use std::{
	collections::HashMap,
	path::Path,
	sync::{Arc, Mutex, Weak},
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
		StringValue,
		Value,
	},
};

use super::{
	link_type::LinkPtr,
	misc::{join_parent, split_sources},
	project::Project,
	starlark_fmt::{format_link_targets, format_strings},
	starlark_link_target::{PtrLinkTarget, StarLinkTarget},
	starlark_project::{StarLinkTargetCache, StarProject},
	static_library::StaticLibrary,
};

#[derive(Clone, Debug, ProvidesStaticType, Allocative)]
pub(super) struct StarStaticLibrary {
	pub parent_project: Weak<Mutex<StarProject>>,
	pub name: String,
	pub sources: Vec<String>,
	pub link_private: Vec<Arc<dyn StarLinkTarget>>,
	pub link_public: Vec<Arc<dyn StarLinkTarget>>,
	pub include_dirs_public: Vec<String>,
	pub include_dirs_private: Vec<String>,
	pub defines_private: Vec<String>,
	pub defines_public: Vec<String>,
	pub link_flags_public: Vec<String>,

	pub generator_vars: Option<String>,

	pub output_name: Option<String>,
}

impl fmt::Display for StarStaticLibrary {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"StaticLibrary{{
  name: "{}",
  sources: [{}],
  link_private: [{}],
  link_public: [{}],
  include_dirs_public: [{}],
  include_dirs_private: [{}],
  defines_private: [{}],
  defines_public: [{}],
  link_flags_public: [{}],
  generator_vars: {},
}}"#,
			self.name,
			format_strings(&self.sources),
			format_link_targets(&self.link_private),
			format_link_targets(&self.link_public),
			format_strings(&self.include_dirs_public),
			format_strings(&self.include_dirs_private),
			format_strings(&self.defines_private),
			format_strings(&self.defines_public),
			format_strings(&self.link_flags_public),
			if self.generator_vars.is_some() {
				"(generated)"
			} else {
				"None"
			},
		)
	}
}

impl StarLinkTarget for StarStaticLibrary {
	fn as_link_target(
		&self,
		parent: Weak<Project>,
		parent_path: &Path,
		ptr: PtrLinkTarget,
		link_map: &mut StarLinkTargetCache,
		gen_name_map: &HashMap<String, OwnedFrozenValue>,
	) -> Result<LinkPtr, String> {
		let arc = Arc::new(self.as_library(parent, parent_path, link_map, gen_name_map)?);
		// let ptr = PtrLinkTarget(arc.clone());
		link_map.insert_static(ptr, arc.clone());
		Ok(LinkPtr::Static(arc))
	}

	fn name(&self) -> String {
		self.name.clone()
	}

	fn public_includes_recursive(&self) -> Vec<String> {
		self.include_dirs_private.clone()
		// for link in &self.link_public {
		// 	public_includes.extend(link.public_includes_recursive());
		// }
		// public_includes
	}
}

impl StarStaticLibrary {
	pub fn as_library(
		&self,
		parent_project: Weak<Project>,
		parent_path: &Path,
		link_map: &mut StarLinkTargetCache,
		gen_name_map: &HashMap<String, OwnedFrozenValue>,
	) -> Result<StaticLibrary, String> {
		Ok(StaticLibrary {
			parent_project: parent_project.clone(),
			name: self.name.clone(),
			sources: split_sources(&self.sources, parent_path)?,
			include_dirs_private: self
				.include_dirs_private
				.iter()
				.map(|x| join_parent(parent_path, x))
				.collect(),
			include_dirs_public: self
				.include_dirs_public
				.iter()
				.map(|x| join_parent(parent_path, x))
				.collect(),
			link_private: self
				.link_private
				.iter()
				.map(|x| {
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lt) = link_map.get(&ptr) {
						Ok(lt)
					} else {
						x.as_link_target(parent_project.clone(), parent_path, ptr, link_map, gen_name_map)
					}
				})
				.collect::<Result<_, _>>()?,
			link_public: self
				.link_public
				.iter()
				.map(|x| {
					let ptr = PtrLinkTarget(x.clone());
					if let Some(lt) = link_map.get(&ptr) {
						Ok(lt)
					} else {
						x.as_link_target(parent_project.clone(), parent_path, ptr, link_map, gen_name_map)
					}
				})
				.collect::<Result<_, _>>()?,
			defines_private: self.defines_private.clone(),
			defines_public: self.defines_public.clone(),
			link_flags_public: self.link_flags_public.clone(),
			generator_vars: match &self.generator_vars {
				None => None,
				Some(id) => match gen_name_map.get(id) {
					Some(x) => Some(x.clone()),
					None => return Err(format!("Could not find generator id in map: {}", id)),
				},
			},
			output_name: self.output_name.clone(),
		})
	}
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarLibraryWrapper(pub(super) Arc<StarStaticLibrary>);

impl fmt::Display for StarLibraryWrapper {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

#[starlark::values::starlark_value(type = "StaticLibrary")]
impl<'v> StarlarkValue<'v> for StarLibraryWrapper {
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
	fn name<'v>(this: &'v StarLibraryWrapper, heap: &'v Heap) -> anyhow::Result<StringValue<'v>> {
		Ok(heap.alloc_str(&format!(":{}", this.0.name)))
	}
}

fn library_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(library_methods_impl)
}
