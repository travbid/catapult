use core::fmt;
use std::{
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
		ProvidesStaticType,
		StarlarkValue,
		StringValue,
		Value,
	},
};

use super::{
	executable::Executable,
	link_type::LinkPtr,
	misc::{join_parent, split_sources},
	project::Project,
	starlark_fmt::{format_link_targets, format_strings},
	starlark_link_target::{PtrLinkTarget, StarLinkTarget},
	starlark_project::{StarLinkTargetCache, StarProject},
};

#[derive(Debug, Allocative)]
pub(super) struct StarExecutable {
	pub parent_project: Weak<Mutex<StarProject>>,

	pub name: String,
	pub sources: Vec<String>,
	pub links: Vec<Arc<dyn StarLinkTarget>>,
	pub include_dirs: Vec<String>,
	pub defines: Vec<String>,
	pub link_flags: Vec<String>,

	pub output_name: Option<String>,
}

impl fmt::Display for StarExecutable {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Executable{{
  name: "{}",
  sources: [{}],
  links: [{}],
  include_dirs: [{}],
  defines: [{}],
  link_flags: [{}],
}}"#,
			self.name,
			format_strings(&self.sources),
			format_link_targets(&self.links),
			format_strings(&self.include_dirs),
			format_strings(&self.defines),
			format_strings(&self.link_flags)
		)
	}
}

impl StarExecutable {
	pub fn as_executable(
		&self,
		parent_project: Weak<Project>,
		parent_path: &Path,
		link_map: &mut StarLinkTargetCache,
	) -> Result<Executable, String> {
		let sources = split_sources(&self.sources, parent_path)?;
		let mut links = Vec::<LinkPtr>::new();
		for link in &self.links {
			let ptr = PtrLinkTarget(link.clone());
			let link_target = match link_map.get(&ptr) {
				Some(x) => x,
				None => link.as_link_target(parent_project.clone(), parent_path, ptr, link_map)?,
			};
			links.push(link_target);
		}
		Ok(Executable {
			parent_project: parent_project.clone(),
			name: self.name.clone(),
			sources,
			links,
			include_dirs: self.include_dirs.iter().map(|x| join_parent(parent_path, x)).collect(),
			defines: self.defines.clone(),
			link_flags: self.link_flags.clone(),
			output_name: self.output_name.clone(),
		})
	}
}

#[starlark_module]
fn executable_methods_impl(builder: &mut MethodsBuilder) {
	fn name<'v>(this: &'v StarExecutableWrapper, heap: &'v Heap) -> anyhow::Result<StringValue<'v>> {
		Ok(heap.alloc_str(&format!(":{}", this.0.name)))
	}
}

fn executable_methods() -> Option<&'static Methods> {
	static RES: MethodsStatic = MethodsStatic::new();
	RES.methods(executable_methods_impl)
}

#[derive(Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarExecutableWrapper(pub(super) Arc<StarExecutable>);

impl fmt::Display for StarExecutableWrapper {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

#[starlark::values::starlark_value(type = "Executable")]
impl<'v> StarlarkValue<'v> for StarExecutableWrapper {
	fn get_methods() -> Option<&'static Methods> {
		println!("Executable::get_methods()");
		executable_methods()
	}

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"include_dirs" => Some(heap.alloc(self.0.include_dirs.clone())),
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

starlark_simple_value!(StarExecutableWrapper);
