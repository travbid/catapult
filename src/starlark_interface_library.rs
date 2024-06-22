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

use crate::{link_type::LinkPtr, misc::join_parent};

use super::{
	interface_library::InterfaceLibrary, //
	project::Project,
	starlark_fmt::{format_link_targets, format_strings},
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
			r#"InterfaceLibrary {{
  name: "{}",
  links: [{}],
  include_dirs: [{}],
  defines: [{}],
  link_flags: [{}],
}}"#,
			self.name,
			format_link_targets(&self.links),
			format_strings(&self.include_dirs),
			format_strings(&self.defines),
			format_strings(&self.link_flags)
		)
	}
}

impl StarLinkTarget for StarIfaceLibrary {
	fn as_link_target(
		&self,
		parent: Weak<Project>,
		parent_path: &Path,
		ptr: PtrLinkTarget,
		link_map: &mut StarLinkTargetCache,
		gen_name_map: &HashMap<String, OwnedFrozenValue>,
	) -> Result<LinkPtr, String> {
		let data = self.as_library(parent, parent_path, link_map, gen_name_map)?;
		let arc = Arc::new(data);
		// let ptr = PtrLinkTarget(arc.clone());
		link_map.insert_interface(ptr, arc.clone());
		Ok(LinkPtr::Interface(arc))
	}

	fn public_includes_recursive(&self) -> Vec<String> {
		let mut public_includes = self.include_dirs.clone();
		for link in &self.links {
			public_includes.extend(link.public_includes_recursive());
		}
		public_includes
	}

	fn name(&self) -> String {
		self.name.clone()
	}
}

impl StarIfaceLibrary {
	pub fn as_library(
		&self,
		parent_project: Weak<Project>,
		parent_path: &Path,
		link_map: &mut StarLinkTargetCache,
		gen_name_map: &HashMap<String, OwnedFrozenValue>,
	) -> Result<InterfaceLibrary, String> {
		Ok(InterfaceLibrary {
			parent_project: parent_project.clone(),
			name: self.name.clone(),
			include_dirs: self.include_dirs.iter().map(|x| join_parent(parent_path, x)).collect(),
			links: self
				.links
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
			defines: self.defines.clone(),
			link_flags: self.link_flags.clone(),
		})
	}
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub(super) struct StarIfaceLibWrapper(pub(super) Arc<StarIfaceLibrary>);

impl fmt::Display for StarIfaceLibWrapper {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

#[starlark::values::starlark_value(type = "InterfaceLibrary")]
impl<'v> StarlarkValue<'v> for StarIfaceLibWrapper {
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

starlark_simple_value!(StarIfaceLibWrapper);

#[starlark_module]
fn library_methods_impl(builder: &mut MethodsBuilder) {
	fn name<'v>(this: &'v StarIfaceLibWrapper, heap: &'v Heap) -> anyhow::Result<StringValue<'v>> {
		Ok(heap.alloc_str(&format!(":{}", this.0.name)))
	}
}

fn library_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(library_methods_impl)
}

// pub(crate) struct InterfaceLibPartial {
// 	base: Arc<StarIfaceLibrary>,
// 	links: Vec<LinkPtrPartial>,
// 	project_path: PathBuf,
// }

// impl InterfaceLibPartial {
// 	fn as_link_target(
// 		lt: Arc<dyn StarLinkTarget>,
// 		project_path: PathBuf,
// 		ptr: PtrLinkTarget,
// 		link_map: &mut StarLinkTargetCache,
// 	) -> Result<LinkPtrPartial, String> {
// 		lt;
// 		let arc = Arc::new(InterfaceLibPartial{
// 			base: lt,
// 			links: lt.links.iter().map,
// 			project_path
// 		});
// 		let ptr = PtrLinkTarget(arc.clone());
// 		link_map.insert_interface(ptr, arc.clone());
// 		Ok(LinkPtrPartial::Interface(arc))
// 	}
// 	pub fn as_library(
// 		base: Arc<StarIfaceLibrary>,
// 		project_path: PathBuf,
// 		link_map: &mut StarLinkTargetCache,
// 	) -> Result<InterfaceLibPartial, String> {
// 		let links = base
// 			.links
// 			.iter()
// 			.map(|x| {
// 				let ptr = PtrLinkTarget(x.clone());
// 				if let Some(lt) = link_map.get(&ptr) {
// 					lt
// 				} else {
// 					as_link_target(x, &project_path, ptr, link_map)?
// 				}
// 			})
// 			.collect()?;
// 		Ok(InterfaceLibPartial { base, links, project_path })
// 	}
// 	pub fn into_interface_library(
// 		&self,
// 		parent: Weak<Project>,
// 		// parent_path: &Path,
// 		// link_map: &mut StarLinkTargetCache,
// 	) -> InterfaceLibrary {
// 		InterfaceLibrary {
// 			parent_project: parent,
// 			name: self.base.name.clone(),
// 			include_dirs: self
// 				.base
// 				.include_dirs
// 				.iter()
// 				.map(|x| join_parent(&self.project_path, x))
// 				.collect(),
// 			links: self.links.into_iter().map(|x| x.into_link_target(&parent)).collect(),
// 			defines: self.base.defines.clone(),
// 			link_flags: self.base.link_flags.clone(),
// 		}
// 	}
// }
