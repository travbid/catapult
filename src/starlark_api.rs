use core::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::{
	environment::GlobalsBuilder,
	eval::Evaluator,
	values::{
		AllocValue, //
		Heap,
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
		list::UnpackList,
	},
};

use crate::{
	starlark_executable::{StarExecutable, StarExecutableWrapper},
	starlark_interface_library::{StarIfaceLibWrapper, StarIfaceLibrary},
	starlark_link_target::{StarLinkTarget, StarLinkTargetRef},
	starlark_object_library::{StarGeneratorVars, StarObjLibWrapper, StarObjectLibrary},
	starlark_project::StarProject,
	starlark_shared_library::{StarSharedLibWrapper, StarSharedLibrary},
	starlark_static_library::{StarStaticLibWrapper, StarStaticLibrary},
};

const GEN_PREFIX: &str = "__gen_";

pub(super) fn err_msg<T>(msg: String) -> Result<T, anyhow::Error> {
	Err(anyhow::Error::msg(msg))
}

#[derive(ProvidesStaticType)]
pub(crate) struct ProjectState {
	pub(crate) project: Arc<Mutex<StarProject>>,
}

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct Context {
	pub compiler_id: String,
}
impl fmt::Display for Context {
	fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
		write!(fmt, "Context")
	}
}
impl<'v> AllocValue<'v> for Context {
	fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
		heap.alloc_simple(self)
	}
}

#[starlark::values::starlark_value(type = "Context")]
impl<'v> StarlarkValue<'v> for Context {
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"compiler_id" => Some(heap.alloc(self.compiler_id.clone())),
			_ => None,
		}
	}
	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		attribute == "compiler_id"
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["compiler_id".to_owned()];
		attrs
	}
}

fn get_link_targets(links: Vec<Value>) -> Result<Vec<Arc<dyn StarLinkTarget>>, anyhow::Error> {
	let mut link_targets = Vec::<Arc<dyn StarLinkTarget>>::with_capacity(links.len());
	for link in links {
		match link.get_type() {
			"InterfaceLibrary" => match StarIfaceLibWrapper::from_value(link) {
				Some(x) => link_targets.push(x.0.clone()),
				None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
			},
			"StaticLibrary" => match StarStaticLibWrapper::from_value(link) {
				Some(x) => link_targets.push(x.0.clone()),
				None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
			},
			"SharedLibrary" => match StarSharedLibWrapper::from_value(link) {
				Some(x) => link_targets.push(x.0.clone()),
				None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
			},
			"ObjectLibrary" => match StarObjLibWrapper::from_value(link) {
				Some(x) => link_targets.push(x.0.clone()),
				None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
			},
			_ => return err_msg(format!("Could not match link {}: {}", link.to_str(), link.get_type())),
		}
	}
	Ok(link_targets)
}

#[starlark::starlark_module]
pub(crate) fn build_api(builder: &mut GlobalsBuilder) {
	fn add_static_library<'v>(
		name: String,
		sources: UnpackList<String>,
		#[starlark(default = Default::default())] link_private: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] link_public: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] include_dirs_private: UnpackList<String>,
		#[starlark(default = Default::default())] include_dirs_public: UnpackList<String>,
		#[starlark(default = Default::default())] defines_private: UnpackList<String>,
		#[starlark(default = Default::default())] defines_public: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags_public: UnpackList<String>,
		generator_vars: Option<Value<'v>>,
		eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarStaticLibWrapper> {
		let state = eval
			.extra
			.unwrap()
			.downcast_ref::<ProjectState>()
			.ok_or(anyhow::anyhow!("No state"))?;
		let lib = Arc::new(StarStaticLibrary {
			parent_project: Arc::downgrade(&state.project),
			name,
			sources: sources.items,
			link_private: get_link_targets(link_private.items)?,
			link_public: get_link_targets(link_public.items)?,
			include_dirs_private: include_dirs_private.items,
			include_dirs_public: include_dirs_public.items,
			defines_private: defines_private.items,
			defines_public: defines_public.items,
			link_flags_public: link_flags_public.items,
			generator_vars: generator_func(generator_vars, eval),
			output_name: None, // TODO(Travers)
		});
		let mut project = state.project.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
		project.link_targets.push(StarLinkTargetRef::Static(lib.clone()));
		project.static_libraries.push(lib.clone());
		Ok(StarStaticLibWrapper(lib))
	}

	fn add_object_library<'v>(
		name: String,
		sources: UnpackList<String>,
		#[starlark(default = Default::default())] link_private: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] link_public: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] include_dirs_private: UnpackList<String>,
		#[starlark(default = Default::default())] include_dirs_public: UnpackList<String>,
		#[starlark(default = Default::default())] defines_private: UnpackList<String>,
		#[starlark(default = Default::default())] defines_public: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags_public: UnpackList<String>,
		generator_vars: Option<Value<'v>>,
		eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarObjLibWrapper> {
		let state = eval
			.extra
			.unwrap()
			.downcast_ref::<ProjectState>()
			.ok_or(anyhow::anyhow!("No state"))?;
		let lib = Arc::new(StarObjectLibrary {
			parent_project: Arc::downgrade(&state.project),
			name,
			sources: sources.items,
			link_private: get_link_targets(link_private.items)?,
			link_public: get_link_targets(link_public.items)?,
			include_dirs_private: include_dirs_private.items,
			include_dirs_public: include_dirs_public.items,
			defines_private: defines_private.items,
			defines_public: defines_public.items,
			link_flags_public: link_flags_public.items,
			generator_vars: generator_func(generator_vars, eval),
			output_name: None, // TODO(Travers)
		});
		let mut project = state.project.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
		project.link_targets.push(StarLinkTargetRef::Object(lib.clone()));
		project.object_libraries.push(lib.clone());
		Ok(StarObjLibWrapper(lib))
	}

	fn add_interface_library<'v>(
		name: String,
		#[starlark(default = Default::default())] link: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] include_dirs: UnpackList<String>,
		#[starlark(default = Default::default())] defines: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags: UnpackList<String>,
		// generator_vars: Option<StarGeneratorVars>,
		eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarIfaceLibWrapper> {
		let state = eval
			.extra
			.unwrap()
			.downcast_ref::<ProjectState>()
			.ok_or(anyhow::anyhow!("No state"))?;
		let lib = Arc::new(StarIfaceLibrary {
			parent_project: Arc::downgrade(&state.project),
			name,
			links: get_link_targets(link.items)?,
			include_dirs: include_dirs.items,
			defines: defines.items,
			link_flags: link_flags.items,
			// generator_vars: generator_func(generator_vars, eval)?,
		});
		let mut project = state.project.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
		project.link_targets.push(StarLinkTargetRef::Interface(lib.clone()));
		project.interface_libraries.push(lib.clone());
		Ok(StarIfaceLibWrapper(lib))
	}

	fn add_shared_library<'v>(
		name: String,
		sources: UnpackList<String>,
		#[starlark(default = Default::default())] link_private: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] link_public: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] include_dirs_private: UnpackList<String>,
		#[starlark(default = Default::default())] include_dirs_public: UnpackList<String>,
		#[starlark(default = Default::default())] defines_private: UnpackList<String>,
		#[starlark(default = Default::default())] defines_public: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags_public: UnpackList<String>,
		generator_vars: Option<Value<'v>>,
		eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarSharedLibWrapper> {
		let state = eval
			.extra
			.unwrap()
			.downcast_ref::<ProjectState>()
			.ok_or(anyhow::anyhow!("No state"))?;
		let lib = Arc::new(StarSharedLibrary {
			parent_project: Arc::downgrade(&state.project),
			name,
			sources: sources.items,
			link_private: get_link_targets(link_private.items)?,
			link_public: get_link_targets(link_public.items)?,
			include_dirs_private: include_dirs_private.items,
			include_dirs_public: include_dirs_public.items,
			defines_private: defines_private.items,
			defines_public: defines_public.items,
			link_flags_public: link_flags_public.items,
			generator_vars: generator_func(generator_vars, eval),
			output_name: None, // TODO(Travers)
		});
		let mut project = state.project.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
		project.link_targets.push(StarLinkTargetRef::Shared(lib.clone()));
		project.shared_libraries.push(lib.clone());
		Ok(StarSharedLibWrapper(lib))
	}

	fn add_executable<'v>(
		name: String,
		sources: UnpackList<String>,
		#[starlark(default = Default::default())] link: UnpackList<Value<'v>>,
		#[starlark(default = Default::default())] include_dirs: UnpackList<String>,
		#[starlark(default = Default::default())] defines: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags: UnpackList<String>,
		generator_vars: Option<Value<'v>>,
		eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarExecutableWrapper> {
		let state = eval
			.extra
			.unwrap()
			.downcast_ref::<ProjectState>()
			.ok_or(anyhow::anyhow!("No state"))?;
		let exe = Arc::new(StarExecutable {
			parent_project: Arc::downgrade(&state.project),
			name,
			sources: sources.items,
			links: get_link_targets(link.items)?,
			include_dirs: include_dirs.items,
			defines: defines.items,
			link_flags: link_flags.items,
			generator_vars: generator_func(generator_vars, eval),
			output_name: None, // TODO(Travers)
		});
		let mut project = state.project.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
		project.executables.push(exe.clone());
		Ok(StarExecutableWrapper(exe))
	}
	fn generator_vars(
		#[starlark(default = Default::default())] sources: UnpackList<String>,
		#[starlark(default = Default::default())] include_dirs: UnpackList<String>,
		#[starlark(default = Default::default())] defines: UnpackList<String>,
		#[starlark(default = Default::default())] link_flags: UnpackList<String>,
		// eval: &mut Evaluator<'v, '_, '_>,
	) -> anyhow::Result<StarGeneratorVars> {
		Ok(StarGeneratorVars {
			sources: sources.items,
			include_dirs: include_dirs.items,
			defines: defines.items,
			link_flags: link_flags.items,
		})
	}
}

fn generator_func<'module>(arg: Option<Value<'module>>, eval: &mut Evaluator<'module, '_, '_>) -> Option<String> {
	match arg {
		None => None,
		Some(x) => {
			if x.is_none() {
				return None;
			}
			let id = String::from(GEN_PREFIX) + &uuid::Uuid::new_v4().to_string();
			eval.module().set(&id, x);
			Some(id)
		}
	}
}
