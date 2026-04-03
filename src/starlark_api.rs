use core::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::{
	environment::GlobalsBuilder,
	eval::{
		Arguments, //
		Evaluator,
		ParametersSpec,
	},
	typing::Ty,
	values::{
		AllocValue,
		FrozenValue,
		Heap,
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		UnpackValue,
		Value,
		list::UnpackList, //
		type_repr::StarlarkTypeRepr,
	},
};

use crate::{
	starlark_executable::{StarExecutable, StarExecutableWrapper},
	starlark_interface_library::{StarIfaceLibWrapper, StarIfaceLibrary},
	starlark_link_target::StarLinkTarget,
	starlark_object_library::{StarGeneratorVars, StarObjLibWrapper, StarObjectLibrary},
	starlark_project::StarProject,
	starlark_static_library::{StarStaticLibWrapper, StarStaticLibrary},
};

const GEN_PREFIX: &str = "__gen_";

pub(super) fn err_msg<T>(msg: String) -> Result<T, anyhow::Error> {
	Err(anyhow::Error::msg(msg))
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
			"ObjectLibrary" => match StarObjLibWrapper::from_value(link) {
				Some(x) => link_targets.push(x.0.clone()),
				None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
			},
			_ => return err_msg(format!("Could not match link {}: {}", link.to_str(), link.get_type())),
		}
	}
	Ok(link_targets)
}

struct ImplAddStaticLibrary {
	signature: ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}

impl starlark::values::function::NativeFunc for ImplAddStaticLibrary {
	fn invoke<'module>(
		&self,
		eval: &mut starlark::eval::Evaluator<'module, '_, '_>,
		parameters: &Arguments<'module, '_>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Option<Value<'module>>; 10] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = required_str("name", args[0])?;
		let sources: Vec<String> = unpack_list("sources", args[1])?;
		let link_private = get_link_targets(unpack_list("link_private", args[2])?)?;
		let link_public = get_link_targets(unpack_list("link_public", args[3])?)?;
		let include_dirs_private: Vec<String> = unpack_list("include_dirs_private", args[4])?;
		let include_dirs_public: Vec<String> = unpack_list("include_dirs_public", args[5])?;
		let defines_private: Vec<String> = unpack_list("defines_private", args[6])?;
		let defines_public: Vec<String> = unpack_list("defines_public", args[7])?;
		let link_flags_public: Vec<String> = unpack_list("link_flags_public", args[8])?;
		let generator_vars = generator_func(args[9], eval);

		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string())?,
		};
		let lib = Arc::new(StarStaticLibrary {
			parent_project: Arc::downgrade(&self.project),
			name,
			sources,
			link_private,
			link_public,
			include_dirs_private,
			include_dirs_public,
			defines_private,
			defines_public,
			link_flags_public,
			generator_vars,
			output_name: None, // TODO(Travers)
		});
		project.static_libraries.push(lib.clone());

		Ok(eval.heap().alloc(StarStaticLibWrapper(lib)))
	}
}

struct ImplAddObjectLibrary {
	signature: ParametersSpec<FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}

impl starlark::values::function::NativeFunc for ImplAddObjectLibrary {
	fn invoke<'module, 'loader, 'extra, 'args>(
		&self,
		eval: &mut starlark::eval::Evaluator<'module, 'loader, '_>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Option<Value<'module>>; 10] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = required_str("name", args[0])?;
		let sources: Vec<String> = unpack_list("sources", args[1])?;
		let link_private = get_link_targets(unpack_list("link_private", args[2])?)?;
		let link_public = get_link_targets(unpack_list("link_public", args[3])?)?;
		let include_dirs_private: Vec<String> = unpack_list("include_dirs_private", args[4])?;
		let include_dirs_public: Vec<String> = unpack_list("include_dirs_public", args[5])?;
		let defines_private: Vec<String> = unpack_list("defines_private", args[6])?;
		let defines_public: Vec<String> = unpack_list("defines_public", args[7])?;
		let link_flags_public: Vec<String> = unpack_list("link_flags_public", args[8])?;
		let generator_vars = generator_func(args[9], eval);

		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string())?,
		};
		let lib = Arc::new(StarObjectLibrary {
			parent_project: Arc::downgrade(&self.project),
			name,
			sources,
			link_private,
			link_public,
			include_dirs_private,
			include_dirs_public,
			defines_private,
			defines_public,
			link_flags_public,
			generator_vars,
			output_name: None, // TODO(Travers)
		});
		project.object_libraries.push(lib.clone());

		Ok(eval.heap().alloc(StarObjLibWrapper(lib)))
	}
}

struct ImplAddInterfaceLibrary {
	signature: ParametersSpec<FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}

impl starlark::values::function::NativeFunc for ImplAddInterfaceLibrary {
	fn invoke<'module, 'loader, 'extra, 'args>(
		&self,
		eval: &mut starlark::eval::Evaluator<'module, 'loader, '_>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Option<Value<'module>>; 5] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = required_str("name", args[0])?;
		let links = get_link_targets(unpack_list("link", args[1])?)?;
		let include_dirs: Vec<String> = unpack_list("include_dirs", args[2])?;
		let defines: Vec<String> = unpack_list("defines", args[3])?;
		let link_flags: Vec<String> = unpack_list("link_flags", args[4])?;

		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string())?,
		};
		let lib = Arc::new(StarIfaceLibrary {
			parent_project: Arc::downgrade(&self.project),
			name,
			links,
			include_dirs,
			defines,
			link_flags,
		});
		project.interface_libraries.push(lib.clone());

		Ok(eval.heap().alloc(StarIfaceLibWrapper(lib)))
	}
}

struct ImplAddExecutable {
	signature: ParametersSpec<FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}

impl starlark::values::function::NativeFunc for ImplAddExecutable {
	fn invoke<'module, 'loader, 'extra, 'args>(
		&self,
		eval: &mut Evaluator<'module, '_, '_>,
		parameters: &Arguments<'module, '_>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [_; 7] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = required_str("name", args[0])?;
		let sources: Vec<String> = unpack_list("sources", args[1])?;
		let links = get_link_targets(unpack_list("link", args[2])?)?;
		let include_dirs: Vec<String> = unpack_list("include_dirs", args[3])?;
		let defines: Vec<String> = unpack_list("defines", args[4])?;
		let link_flags: Vec<String> = unpack_list("link_flags", args[5])?;
		let generator_vars = generator_func(args[6], eval);

		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string())?,
		};
		let exe = Arc::new(StarExecutable {
			parent_project: Arc::downgrade(&self.project),
			name,
			sources,
			links,
			include_dirs,
			defines,
			link_flags,
			generator_vars,
			output_name: None, // TODO(Travers)
		});
		project.executables.push(exe.clone());
		Ok(eval.heap().alloc(StarExecutableWrapper(exe)))
	}
}

struct ImplGeneratorVar {
	signature: ParametersSpec<FrozenValue>,
}

impl starlark::values::function::NativeFunc for ImplGeneratorVar {
	fn invoke<'module, 'loader, 'extra, 'args>(
		&self,
		eval: &mut starlark::eval::Evaluator<'module, 'loader, '_>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Option<Value<'module>>; 4] = self.signature.collect_into(parameters, eval.heap())?;
		let ret = StarGeneratorVars {
			sources: unpack_list("sources", args[0])?,
			include_dirs: unpack_list("include_dirs", args[1])?,
			defines: unpack_list("defines", args[2])?,
			link_flags: unpack_list("link_flags", args[3])?,
		};
		Ok(eval.heap().alloc(ret))
	}
}

pub(crate) fn build_api(project: &Arc<Mutex<StarProject>>, builder: &mut GlobalsBuilder) {
	use starlark::{
		__derive_refs::{
			components::NativeCallableComponents,
			param_spec::{NativeCallableParam, NativeCallableParamDefaultValue, NativeCallableParamSpec},
		},
		eval::{
			ParametersSpec, ParametersSpecParam,
			ParametersSpecParam::{Defaulted, Optional, Required},
		},
	};
	fn params<'a, 'c, const N: usize>(
		lst: [(&'static str, Ty, ParametersSpecParam<FrozenValue>); N],
	) -> (Vec<(&'static str, ParametersSpecParam<FrozenValue>)>, Vec<NativeCallableParam>) {
		let pair_iter = lst.into_iter().map(|(name, ty, spec)| {
			let native_callable_param = match spec {
				Required => NativeCallableParam { name, ty, required: None },
				Optional => NativeCallableParam {
					name,
					ty,
					required: Some(NativeCallableParamDefaultValue::Optional),
				},
				Defaulted(v) => NativeCallableParam {
					name,
					ty,
					required: Some(NativeCallableParamDefaultValue::Value(v)),
				},
			};
			((name, spec), native_callable_param)
		});
		let unzipped = pair_iter.unzip();
		unzipped
	}
	fn empty_list() -> FrozenValue {
		FrozenValue::new_empty_list()
	}

	{
		let function_name = "add_static_library";
		let params = params([
			("name", <&str>::starlark_type_repr(), Required),
			("sources", <Vec<&str>>::starlark_type_repr(), Required),
			("link_private", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_public", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs_private", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines_private", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_flags_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("generator_vars", <StarGeneratorVars>::starlark_type_repr(), Optional),
		]);
		let signature = ParametersSpec::<FrozenValue>::new_parts(function_name, [], [], false, params.0, false);
		let components = NativeCallableComponents {
			speculative_exec_safe: false,
			rust_docstring: None,
			param_spec: NativeCallableParamSpec {
				pos_only: vec![],
				pos_or_named: vec![],
				args: None,
				named_only: params.1,
				kwargs: None,
			},
			return_type: <StarStaticLibWrapper>::starlark_type_repr(),
		};
		builder.set_function(
			function_name,
			components,
			None,
			Some(StarStaticLibWrapper::starlark_type_repr()),
			None,
			ImplAddStaticLibrary { signature, project: project.clone() },
		);
	}
	{
		let function_name = "add_object_library";
		let params = params([
			("name", <&str>::starlark_type_repr(), Required),
			("sources", <Vec<&str>>::starlark_type_repr(), Required),
			("link_private", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_public", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs_private", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines_private", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_flags_public", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("generator_vars", <StarGeneratorVars>::starlark_type_repr(), Optional),
		]);
		let signature = ParametersSpec::<FrozenValue>::new_parts(function_name, [], [], false, params.0, false);
		let components = NativeCallableComponents {
			speculative_exec_safe: false,
			rust_docstring: None,
			param_spec: NativeCallableParamSpec {
				pos_only: vec![],
				pos_or_named: vec![],
				args: None,
				named_only: params.1,
				kwargs: None,
			},
			return_type: <StarObjLibWrapper>::starlark_type_repr(),
		};
		builder.set_function(
			function_name,
			components,
			None,
			Some(StarObjLibWrapper::starlark_type_repr()),
			None,
			ImplAddObjectLibrary { signature, project: project.clone() },
		);
	}
	{
		let function_name = "add_interface_library";
		let params = params([
			("name", <&str>::starlark_type_repr(), Required),
			("link", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_flags", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
		]);
		let signature = ParametersSpec::<FrozenValue>::new_parts(function_name, [], [], false, params.0, false);
		let components = NativeCallableComponents {
			speculative_exec_safe: false,
			rust_docstring: None,
			param_spec: NativeCallableParamSpec {
				pos_only: vec![],
				pos_or_named: vec![],
				args: None,
				named_only: params.1,
				kwargs: None,
			},
			return_type: <Value>::starlark_type_repr(),
		};
		builder.set_function(
			function_name,
			components,
			None,
			Some(StarIfaceLibWrapper::starlark_type_repr()),
			None,
			ImplAddInterfaceLibrary { signature, project: project.clone() },
		);
	}
	{
		let function_name = "add_executable";
		let params = params([
			("name", <&str>::starlark_type_repr(), Required),
			("sources", <Vec<&str>>::starlark_type_repr(), Required),
			("link", <Vec<Value>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_flags", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("generator_vars", <StarGeneratorVars>::starlark_type_repr(), Optional),
		]);
		let signature = ParametersSpec::<FrozenValue>::new_parts(function_name, [], [], false, params.0, false);
		let components = NativeCallableComponents {
			speculative_exec_safe: false,
			rust_docstring: None,
			param_spec: NativeCallableParamSpec {
				pos_only: vec![],
				pos_or_named: vec![],
				args: None,
				named_only: params.1,
				kwargs: None,
			},
			return_type: <Value>::starlark_type_repr(),
		};
		builder.set_function(
			function_name,
			components,
			None,
			Some(StarExecutableWrapper::starlark_type_repr()),
			None,
			ImplAddExecutable { signature, project: project.clone() },
		);
	}
	{
		let function_name = "generator_vars";
		let params = params([
			("sources", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("include_dirs", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("defines", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
			("link_flags", <Vec<&str>>::starlark_type_repr(), Defaulted(empty_list())),
		]);
		let signature = ParametersSpec::<FrozenValue>::new_parts(function_name, [], [], false, params.0, false);
		let components = NativeCallableComponents {
			speculative_exec_safe: false,
			rust_docstring: None,
			param_spec: NativeCallableParamSpec {
				pos_only: vec![],
				pos_or_named: vec![],
				args: None,
				named_only: params.1,
				kwargs: None,
			},
			return_type: <StarGeneratorVars>::starlark_type_repr(),
		};
		builder.set_function(
			function_name,
			components,
			None,
			Some(StarGeneratorVars::starlark_type_repr()),
			None,
			ImplGeneratorVar { signature },
		);
	}
}

fn required_str<'a>(name: &str, arg: Option<Value<'a>>) -> anyhow::Result<String> {
	let x = arg.ok_or_else(|| anyhow::anyhow!("Missing required parameter: {}", name))?;
	let s = x
		.unpack_str()
		.ok_or_else(|| anyhow::anyhow!("{} must be a string", name))?;
	Ok(s.to_owned())
}

fn unpack_list<'a, T: UnpackValue<'a>>(name: &str, arg: Option<Value<'a>>) -> anyhow::Result<Vec<T>> {
	let x = arg.ok_or_else(|| anyhow::anyhow!("Missing required parameter: {}", name))?;
	match UnpackList::<T>::unpack_value(x) {
		Ok(Some(list)) => Ok(list.items),
		Ok(None) => Err(anyhow::anyhow!("Incorrect parameter type for {}: expected a list", name)),
		Err(e) => Err(anyhow::anyhow!("Error unpacking {}: {}", name, e)),
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
