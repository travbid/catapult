use core::{cell::Cell, fmt};
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::{
	environment::GlobalsBuilder,
	eval::Arguments,
	typing::Ty,
	values::{
		list::UnpackList,
		type_repr::StarlarkTypeRepr, //
		AllocValue,
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
	starlark_object_library::{StarObjLibWrapper, StarObjectLibrary},
	starlark_project::StarProject,
	starlark_static_library::{StarLibraryWrapper, StarStaticLibrary},
};

pub(super) fn err_msg<T>(msg: String) -> Result<T, anyhow::Error> {
	Err(anyhow::Error::msg(msg))
}

fn to_vec_strs(paths: &[&str]) -> Vec<String> {
	paths.iter().copied().map(String::from).collect()
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
			"StaticLibrary" => match StarLibraryWrapper::from_value(link) {
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
	signature: starlark::eval::ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}
impl ImplAddStaticLibrary {
	#[allow(clippy::too_many_arguments)]
	fn add_static_library_impl(
		&self,
		name: &str,
		sources: Vec<&str>,
		link_private: Vec<Value>,
		link_public: Vec<Value>,
		include_dirs_private: Vec<&str>,
		include_dirs_public: Vec<&str>,
		defines_public: Vec<&str>,
		link_flags_public: Vec<&str>,
		// list_or_lambda: Arc<ListOrLambdaFrozen>,
	) -> anyhow::Result<Arc<StarStaticLibrary>> {
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let lib = Arc::new(StarStaticLibrary {
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			sources: to_vec_strs(&sources),
			link_private: get_link_targets(link_private)?,
			link_public: get_link_targets(link_public)?,
			include_dirs_private: to_vec_strs(&include_dirs_private),
			include_dirs_public: to_vec_strs(&include_dirs_public),
			defines_public: defines_public.into_iter().map(String::from).collect(),
			link_flags_public: link_flags_public.into_iter().map(String::from).collect(),
			output_name: None, // TODO(Travers)
		});
		project.static_libraries.push(lib.clone());
		Ok(lib)
	}
}

impl starlark::values::function::NativeFunc for ImplAddStaticLibrary {
	fn invoke<'v>(
		&self,
		eval: &mut starlark::eval::Evaluator<'v, '_>,
		parameters: &Arguments<'v, '_>,
	) -> Result<starlark::values::Value<'v>, starlark::Error> {
		let args: [Cell<Option<Value<'v>>>; 8] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_static_library_impl(
			Arguments::check_required("name", args[0].get())?,
			required_list("sources", args[1].get())?,
			optional_list("link_private", args[2].get())?,
			optional_list("link_public", args[3].get())?,
			optional_list("include_dirs_private", args[4].get())?,
			optional_list("include_dirs_public", args[5].get())?,
			optional_list("defines_public", args[6].get())?,
			optional_list("link_flags_public", args[7].get())?,
		)?;
		Ok(eval.heap().alloc(StarLibraryWrapper(v)))
	}
}

struct ImplAddObjectLibrary {
	signature: starlark::eval::ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}
impl ImplAddObjectLibrary {
	#[allow(clippy::too_many_arguments)]
	fn add_object_library_impl(
		&self,
		name: &str,
		sources: Vec<&str>,
		link_private: Vec<Value>,
		link_public: Vec<Value>,
		include_dirs_public: Vec<&str>,
		include_dirs_private: Vec<&str>,
		defines_public: Vec<&str>,
		link_flags_public: Vec<&str>,
		// list_or_lambda: Arc<ListOrLambdaFrozen>,
	) -> anyhow::Result<Arc<StarObjectLibrary>> {
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let lib = Arc::new(StarObjectLibrary {
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			sources: to_vec_strs(&sources),
			link_private: get_link_targets(link_private)?,
			link_public: get_link_targets(link_public)?,
			include_dirs_private: to_vec_strs(&include_dirs_private),
			include_dirs_public: to_vec_strs(&include_dirs_public),
			defines_public: defines_public.into_iter().map(String::from).collect(),
			link_flags_public: link_flags_public.into_iter().map(String::from).collect(),
			output_name: None, // TODO(Travers)
		});
		project.object_libraries.push(lib.clone());
		Ok(lib)
	}
}

impl starlark::values::function::NativeFunc for ImplAddObjectLibrary {
	fn invoke<'v>(
		&self,
		eval: &mut starlark::eval::Evaluator<'v, '_>,
		parameters: &Arguments<'v, '_>,
	) -> Result<starlark::values::Value<'v>, starlark::Error> {
		let args: [Cell<Option<Value<'v>>>; 8] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_object_library_impl(
			Arguments::check_required("name", args[0].get())?,
			required_list("sources", args[1].get())?,
			optional_list("link_private", args[2].get())?,
			optional_list("link_public", args[3].get())?,
			optional_list("include_dirs_private", args[5].get())?,
			optional_list("include_dirs_public", args[4].get())?,
			optional_list("defines_public", args[6].get())?,
			optional_list("link_flags_public", args[7].get())?,
		)?;
		Ok(eval.heap().alloc(StarObjLibWrapper(v)))
	}
}

struct ImplAddInterfaceLibrary {
	signature: starlark::eval::ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}
impl ImplAddInterfaceLibrary {
	fn add_interface_library_impl(
		&self,
		name: &str,
		links: Vec<Value>,
		include_dirs: Vec<&str>,
		defines: Vec<&str>,
		link_flags: Vec<&str>,
	) -> anyhow::Result<Arc<StarIfaceLibrary>> {
		let links = get_link_targets(links)?;
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let lib = Arc::new(StarIfaceLibrary {
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			links,
			include_dirs: to_vec_strs(&include_dirs),
			defines: defines.into_iter().map(String::from).collect(),
			link_flags: link_flags.into_iter().map(String::from).collect(),
		});
		project.interface_libraries.push(lib.clone());
		Ok(lib)
	}
}

impl starlark::values::function::NativeFunc for ImplAddInterfaceLibrary {
	fn invoke<'v>(
		&self,
		eval: &mut starlark::eval::Evaluator<'v, '_>,
		parameters: &Arguments<'v, '_>,
	) -> Result<starlark::values::Value<'v>, starlark::Error> {
		let args: [Cell<Option<Value<'v>>>; 5] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_interface_library_impl(
			Arguments::check_required("name", args[0].get())?,
			optional_list("link", args[1].get())?,
			optional_list("include_dirs", args[2].get())?,
			optional_list("defines", args[3].get())?,
			optional_list("link_flags", args[4].get())?,
			// listorlambda,
		)?;
		Ok(eval.heap().alloc(StarIfaceLibraryWrapper(v)))
	}
}

struct ImplAddExecutable {
	signature: starlark::eval::ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}
impl ImplAddExecutable {
	fn add_executable_impl(
		&self,
		name: &str,
		sources: Vec<&str>,
		links: Vec<Value>,
		include_dirs: Vec<String>,
		defines: Vec<String>,
		link_flags: Vec<String>,
	) -> anyhow::Result<StarExecutableWrapper> {
		let exe_links = get_link_targets(links)?;
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let exe = Arc::new(StarExecutable {
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			sources: to_vec_strs(&sources),
			links: exe_links,
			include_dirs,
			defines,
			link_flags,
			output_name: None, // TODO(Travers)
		});
		project.executables.push(exe.clone());
		Ok(StarExecutableWrapper(exe))
	}
}
impl starlark::values::function::NativeFunc for ImplAddExecutable {
	fn invoke<'v>(
		&self,
		eval: &mut starlark::eval::Evaluator<'v, '_>,
		parameters: &Arguments<'v, '_>,
	) -> Result<starlark::values::Value<'v>, starlark::Error> {
		let args: [_; 6] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_executable_impl(
			Arguments::check_required("name", args[0].get())?,
			required_list("sources", args[1].get())?,
			optional_list("links", args[2].get())?,
			optional_list("include_dirs", args[3].get())?,
			optional_list("defines", args[4].get())?,
			optional_list("link_flags", args[5].get())?,
		)?;
		Ok(eval.heap().alloc(v))
	}
}

pub(crate) fn build_api(project: &Arc<Mutex<StarProject>>, builder: &mut GlobalsBuilder) {
	{
		let function_name = "add_static_library";
		let mut sig_builder = starlark::eval::ParametersSpec::new(function_name.to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link_private");
		sig_builder.optional("link_public");
		sig_builder.optional("include_dirs_private");
		sig_builder.optional("include_dirs_public");
		sig_builder.optional("defines_public");
		sig_builder.optional("link_flags_public");
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
			function_name,
			false,
			documentation,
			None,
			Some(StarLibraryWrapper::starlark_type_repr()),
			None,
			ImplAddStaticLibrary { signature, project: project.clone() },
		);
	}
	{
		let function_name = "add_object_library";
		let mut sig_builder = starlark::eval::ParametersSpec::new(function_name.to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link_private");
		sig_builder.optional("link_public");
		sig_builder.optional("include_dirs_private");
		sig_builder.optional("include_dirs_public");
		sig_builder.optional("defines_public");
		sig_builder.optional("link_flags_public");
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
			function_name,
			false,
			documentation,
			None,
			Some(StarLibraryWrapper::starlark_type_repr()),
			None,
			ImplAddObjectLibrary { signature, project: project.clone() },
		);
	}
	{
		let mut sig_builder = starlark::eval::ParametersSpec::new("add_interface_library".to_owned());
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
		let mut sig_builder = starlark::eval::ParametersSpec::new("add_executable".to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link");
		sig_builder.optional("include_dirs");
		sig_builder.optional("defines");
		sig_builder.optional("link_flags");
		let signature = sig_builder.finish();

		let documentation = {
			let parameter_types = Vec::<Ty>::from([
				<&str>::starlark_type_repr(),
				<Vec<&str>>::starlark_type_repr(),
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
			"add_executable",
			false,
			documentation,
			None,
			Some(StarExecutableWrapper::starlark_type_repr()),
			None,
			ImplAddExecutable { signature, project: project.clone() },
		);
	}
}

fn required_list<'a, T: UnpackValue<'a>>(name: &str, arg: Option<Value<'a>>) -> anyhow::Result<Vec<T>> {
	let x = arg.ok_or_else(|| starlark::values::ValueError::MissingRequired(name.to_owned()))?;
	let items = UnpackList::unpack_named_param(x, name)?.items;
	Ok(items)
}

fn optional_list<'a, T: UnpackValue<'a>>(name: &str, arg: Option<Value<'a>>) -> anyhow::Result<Vec<T>> {
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
