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
	},
};

use super::{
	executable::Executable,
	misc::{is_c_source, is_cpp_source},
	project::Project,
	starlark_link_target::{PtrLinkTarget, StarLinkTarget},
	starlark_project::{StarLinkTargetCache, StarProject},
	target::LinkTarget,
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
		let mut sources = self.sources.join(",\n      ");
		if self.sources.len() > 1 {
			sources = String::from("\n      ") + &sources + ",\n   "
		}
		write!(
			f,
			r#"StarExecutable{{
   name: {},
   sources: [{:?}],
}}"#,
			self.name, sources
		)
	}
}

impl StarExecutable {
	pub fn as_executable(&self, parent_project: Weak<Project>, link_map: &mut StarLinkTargetCache) -> Executable {
		let mut links = Vec::<Arc<dyn LinkTarget>>::new();
		for link in &self.links {
			let ptr = PtrLinkTarget(link.clone());
			let link_target = match link_map.get(&ptr) {
				Some(x) => x,
				None => {
					let x = link.as_link_target(parent_project.clone(), ptr, link_map);
					x
				}
			};
			links.push(link_target);
		}
		Executable {
			parent_project,
			name: self.name.clone(),
			c_sources: self.sources.iter().filter(is_c_source).map(String::from).collect(),
			cpp_sources: self.sources.iter().filter(is_cpp_source).map(String::from).collect(),
			links,
			include_dirs: self.include_dirs.clone(),
			defines: self.defines.clone(),
			link_flags: self.link_flags.clone(),
			output_name: self.output_name.clone(),
		}
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

impl<'v> StarlarkValue<'v> for StarExecutableWrapper {
	starlark_type!("Executable");
	fn get_methods() -> Option<&'static Methods> {
		println!("Executable::get_methods()");
		executable_methods()
	}
}

starlark_simple_value!(StarExecutableWrapper);
