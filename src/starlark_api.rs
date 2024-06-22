use core::{cell::Cell, fmt};
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
		list::UnpackList, //
		type_repr::StarlarkTypeRepr,
		AllocValue,
		FrozenValue,
		Heap,
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		UnpackValue,
		Value,
	},
};

use crate::{
	starlark_executable::{StarExecutable, StarExecutableWrapper},
	starlark_interface_library::{StarIfaceLibrary, StarIfaceLibraryWrapper},
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
			"InterfaceLibrary" => match StarIfaceLibraryWrapper::from_value(link) {
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
		eval: &mut starlark::eval::Evaluator<'module, '_>,
		parameters: &Arguments<'module, '_>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Cell<Option<Value<'module>>>; 10] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = Arguments::check_required("name", args[0].get())?;
		let sources: Vec<String> = required_list("sources", args[1].get())?;
		let link_private = get_link_targets(optional_list("link_private", args[2].get())?)?;
		let link_public = get_link_targets(optional_list("link_public", args[3].get())?)?;
		let include_dirs_private: Vec<String> = optional_list("include_dirs_private", args[4].get())?;
		let include_dirs_public: Vec<String> = optional_list("include_dirs_public", args[5].get())?;
		let defines_private: Vec<String> = optional_list("defines_private", args[6].get())?;
		let defines_public: Vec<String> = optional_list("defines_public", args[7].get())?;
		let link_flags_public: Vec<String> = optional_list("link_flags_public", args[8].get())?;
		let generator_vars = generator_func(args[9].get(), eval);

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
		eval: &mut starlark::eval::Evaluator<'module, 'loader>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Cell<Option<Value<'module>>>; 10] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = Arguments::check_required("name", args[0].get())?;
		let sources: Vec<String> = required_list("sources", args[1].get())?;
		let link_private = get_link_targets(optional_list("link_private", args[2].get())?)?;
		let link_public = get_link_targets(optional_list("link_public", args[3].get())?)?;
		let include_dirs_private: Vec<String> = optional_list("include_dirs_private", args[4].get())?;
		let include_dirs_public: Vec<String> = optional_list("include_dirs_public", args[5].get())?;
		let defines_private: Vec<String> = optional_list("defines_private", args[6].get())?;
		let defines_public: Vec<String> = optional_list("defines_public", args[7].get())?;
		let link_flags_public: Vec<String> = optional_list("link_flags_public", args[8].get())?;
		let generator_vars = generator_func(args[9].get(), eval);

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
		eval: &mut starlark::eval::Evaluator<'module, 'loader>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Cell<Option<Value<'module>>>; 5] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = Arguments::check_required("name", args[0].get())?;
		let links = get_link_targets(optional_list("link", args[1].get())?)?;
		let include_dirs: Vec<String> = optional_list("include_dirs", args[2].get())?;
		let defines: Vec<String> = optional_list("defines", args[3].get())?;
		let link_flags: Vec<String> = optional_list("link_flags", args[4].get())?;

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

		Ok(eval.heap().alloc(StarIfaceLibraryWrapper(lib)))
	}
}

struct ImplAddExecutable {
	signature: ParametersSpec<FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}

impl starlark::values::function::NativeFunc for ImplAddExecutable {
	fn invoke<'module, 'loader, 'extra, 'args>(
		&self,
		eval: &mut Evaluator<'module, '_>,
		parameters: &Arguments<'module, '_>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [_; 7] = self.signature.collect_into(parameters, eval.heap())?;

		let name: String = Arguments::check_required("name", args[0].get())?;
		let sources: Vec<String> = required_list("sources", args[1].get())?;
		let links = get_link_targets(optional_list("link", args[2].get())?)?;
		let include_dirs: Vec<String> = optional_list("include_dirs", args[3].get())?;
		let defines: Vec<String> = optional_list("defines", args[4].get())?;
		let link_flags: Vec<String> = optional_list("link_flags", args[5].get())?;
		let generator_vars = generator_func(args[6].get(), eval);

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
		eval: &mut starlark::eval::Evaluator<'module, 'loader>,
		parameters: &Arguments<'module, 'args>,
	) -> Result<starlark::values::Value<'module>, starlark::Error> {
		let args: [Cell<Option<Value<'module>>>; 4] = self.signature.collect_into(parameters, eval.heap())?;
		let ret = StarGeneratorVars {
			sources: optional_list("sources", args[0].get())?,
			include_dirs: optional_list("include_dirs", args[1].get())?,
			defines: optional_list("defines", args[2].get())?,
			link_flags: optional_list("link_flags", args[3].get())?,
		};
		Ok(eval.heap().alloc(ret))
	}
}

pub(crate) fn build_api(project: &Arc<Mutex<StarProject>>, builder: &mut GlobalsBuilder) {
	{
		let function_name = "add_static_library";
		let mut sig_builder = ParametersSpec::new(function_name.to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link_private");
		sig_builder.optional("link_public");
		sig_builder.optional("include_dirs_private");
		sig_builder.optional("include_dirs_public");
		sig_builder.optional("defines_private");
		sig_builder.optional("defines_public");
		sig_builder.optional("link_flags_public");
		sig_builder.optional("generator_vars");
		let signature = sig_builder.finish();
		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<&str>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<StarGeneratorVars>::starlark_type_repr(),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: <StarStaticLibWrapper>::starlark_type_repr(),
				as_type: None,
			}
		};
		builder.set_function(
			function_name,
			false,
			documentation,
			None,
			Some(StarStaticLibWrapper::starlark_type_repr()),
			None,
			ImplAddStaticLibrary { signature, project: project.clone() },
		);
	}
	{
		let function_name = "add_object_library";
		let mut sig_builder = ParametersSpec::new(function_name.to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link_private");
		sig_builder.optional("link_public");
		sig_builder.optional("include_dirs_private");
		sig_builder.optional("include_dirs_public");
		sig_builder.optional("defines_private");
		sig_builder.optional("defines_public");
		sig_builder.optional("link_flags_public");
		sig_builder.optional("generator_vars");
		let signature = sig_builder.finish();
		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<&str>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<StarGeneratorVars>::starlark_type_repr(),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: <StarObjLibWrapper>::starlark_type_repr(),
				as_type: None,
			}
		};
		builder.set_function(
			function_name,
			false,
			documentation,
			None,
			Some(StarObjLibWrapper::starlark_type_repr()),
			None,
			ImplAddObjectLibrary { signature, project: project.clone() },
		);
	}
	{
		let mut sig_builder = ParametersSpec::new("add_interface_library".to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.optional("link");
		sig_builder.optional("include_dirs");
		sig_builder.optional("defines");
		sig_builder.optional("link_flags");
		let signature = sig_builder.finish();
		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<&str>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: <Value>::starlark_type_repr(),
				as_type: None,
			}
		};
		builder.set_function(
			"add_interface_library",
			false,
			documentation,
			None,
			Some(StarIfaceLibraryWrapper::starlark_type_repr()),
			None,
			ImplAddInterfaceLibrary { signature, project: project.clone() },
		);
	}
	{
		let mut sig_builder = ParametersSpec::new("add_executable".to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link");
		sig_builder.optional("include_dirs");
		sig_builder.optional("defines");
		sig_builder.optional("link_flags");
		sig_builder.optional("generator_vars");
		let signature = sig_builder.finish();

		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<&str>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<Value>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<StarGeneratorVars>::starlark_type_repr(),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: <Value>::starlark_type_repr(),
				as_type: None,
			}
		};

		builder.set_function(
			"add_executable",
			false,
			documentation,
			None,
			Some(StarExecutableWrapper::starlark_type_repr()),
			None,
			ImplAddExecutable { signature, project: project.clone() },
		);
	}
	{
		let function_name = "generator_vars";
		let mut sig_builder = ParametersSpec::new(function_name.to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.optional("sources");
		sig_builder.optional("include_dirs");
		sig_builder.optional("defines");
		sig_builder.optional("link_flags");
		let signature = sig_builder.finish();
		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: <StarGeneratorVars>::starlark_type_repr(),
				as_type: None,
			}
		};
		builder.set_function(
			function_name,
			false,
			documentation,
			None,
			Some(StarGeneratorVars::starlark_type_repr()),
			None,
			ImplGeneratorVar { signature },
		);
	}
}

fn required_list<'a, T: UnpackValue<'a>>(name: &str, arg: Option<Value<'a>>) -> anyhow::Result<Vec<T>> {
	let x = arg.ok_or_else(|| starlark::values::ValueError::MissingRequired(name.to_owned()))?;
	let items = UnpackList::unpack_named_param(x, name)?.items;
	Ok(items)
}

fn optional_list<'module, T: UnpackValue<'module>>(name: &str, arg: Option<Value<'module>>) -> anyhow::Result<Vec<T>> {
	match arg {
		None => Ok(Vec::new()),
		Some(x) => Ok(UnpackList::unpack_value(x)
			.ok_or_else::<anyhow::Error, _>(|| {
				starlark::values::ValueError::IncorrectParameterTypeNamedWithExpected(
					name.to_owned(),
					UnpackList::<Value>::expected(),
					x.get_type().to_owned(),
				)
				.into()
			})?
			.items),
	}
}

fn generator_func<'module>(arg: Option<Value<'module>>, eval: &mut Evaluator<'module, '_>) -> Option<String> {
	match arg {
		None => None,
		Some(x) => {
			let id = String::from(GEN_PREFIX) + &uuid::Uuid::new_v4().to_string();
			eval.module().set(&id, x);
			Some(id)
		}
	}
}
