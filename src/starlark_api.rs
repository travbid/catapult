use core::{cell::Cell, fmt};
use std::{
	collections::HashMap, //
	path::Path,
	sync::{Arc, Mutex},
};

use allocative::Allocative;
use starlark::{
	docs::DocType,
	environment::GlobalsBuilder,
	eval::Arguments,
	starlark_type,
	values::{
		type_repr::StarlarkTypeRepr, //
		AllocValue,
		Heap,
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

use crate::{
	starlark_executable::{StarExecutable, StarExecutableWrapper},
	starlark_library::{StarLibrary, StarLibraryWrapper},
	starlark_link_target::StarLinkTarget,
	starlark_project::StarProject,
};

pub(super) fn err_msg<T>(msg: String) -> Result<T, anyhow::Error> {
	Err(anyhow::Error::msg(msg))
}

fn to_path_strs(paths: &[&str], project_path: &Path) -> Vec<String> {
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
impl<'v> StarlarkValue<'v> for Context {
	starlark_type!("Context");
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		println!("Context::get_attr({attribute})");
		match attribute {
			"compiler_id" => Some(heap.alloc(self.compiler_id.clone())),
			_ => None,
		}
	}
	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		println!("Context::has_attr({attribute})");
		attribute == "compiler_id"
	}

	fn dir_attr(&self) -> Vec<String> {
		println!("Context::dir_attr()");
		let attrs = vec!["compiler_id".to_owned()];
		attrs
	}
}

struct ImplAddLibrary {
	signature: starlark::eval::ParametersSpec<starlark::values::FrozenValue>,
	project: Arc<Mutex<StarProject>>,
}
impl ImplAddLibrary {
	fn add_library_impl(
		&self,
		name: &str,
		sources: Vec<&str>,
		link_private: Vec<Value>,
		include_dirs_public: Vec<&str>,
		include_dirs_private: Vec<&str>,
		defines_public: Vec<&str>,
		link_flags_public: Vec<&str>,
		// list_or_lambda: Arc<ListOrLambdaFrozen>,
	) -> anyhow::Result<Arc<StarLibrary>> {
		let mut private_links = Vec::<Arc<dyn StarLinkTarget>>::with_capacity(link_private.len());
		for link in link_private {
			match link.get_type() {
				"Library" => match StarLibraryWrapper::from_value(link) {
					Some(x) => private_links.push(x.0.clone()),
					None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
				},
				_ => return err_msg(format!("Could not match \"link\" {}: {}", link.to_str(), link.get_type())),
			}
		}
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let lib = Arc::new(StarLibrary {
			// parent_project: Arc::new_cyclic(|x| true),
			// parent_project: Arc::downgrade(&self.project),
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			sources: to_path_strs(&sources, &project.path),
			private_links,
			include_dirs_public: to_path_strs(&include_dirs_public, &project.path),
			include_dirs_private: to_path_strs(&include_dirs_private, &project.path),
			defines_public: defines_public.into_iter().map(String::from).collect(),
			link_flags_public: link_flags_public.into_iter().map(String::from).collect(),
			output_name: None, // TODO(Travers)
		});
		project.libraries.push(lib.clone());
		Ok(lib)
	}
}

impl starlark::values::function::NativeFunc for ImplAddLibrary {
	fn invoke<'v>(
		&self,
		eval: &mut starlark::eval::Evaluator<'v, '_>,
		parameters: &Arguments<'v, '_>,
	) -> anyhow::Result<starlark::values::Value<'v>> {
		let args: [Cell<Option<Value<'v>>>; 7] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_library_impl(
			Arguments::check_required("name", args[0].get())?,
			Arguments::check_required("sources", args[1].get())?,
			Arguments::check_optional("link_private", args[2].get())?.unwrap_or_default(),
			Arguments::check_optional("include_dirs_public", args[3].get())?.unwrap_or_default(),
			Arguments::check_optional("include_dirs_private", args[4].get())?.unwrap_or_default(),
			Arguments::check_optional("defines_public", args[5].get())?.unwrap_or_default(),
			Arguments::check_optional("link_flags_public", args[6].get())?.unwrap_or_default(),
			// listorlambda,
		)?;
		Ok(eval.heap().alloc(StarLibraryWrapper(v)))
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
		let mut exe_links = Vec::<Arc<dyn StarLinkTarget>>::with_capacity(sources.len());
		for link in links {
			match link.get_type() {
				"Library" => match StarLibraryWrapper::from_value(link) {
					Some(x) => exe_links.push(x.0.clone()),
					None => return err_msg(format!("Could not unpack \"link\" {}", link.get_type())),
				},
				_ => return err_msg(format!("Could not match \"link\" {}: {}", link.to_str(), link.get_type())),
			}
		}
		let mut project = match self.project.lock() {
			Ok(x) => x,
			Err(e) => return err_msg(e.to_string()),
		};
		let exe = Arc::new(StarExecutable {
			parent_project: Arc::downgrade(&self.project),
			name: String::from(name),
			sources: to_path_strs(&sources, &project.path),
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
	) -> anyhow::Result<starlark::values::Value<'v>> {
		let args: [_; 6] = self.signature.collect_into(parameters, eval.heap())?;
		let v = self.add_executable_impl(
			Arguments::check_required("name", args[0].get())?,
			Arguments::check_required("sources", args[1].get())?,
			Arguments::check_optional("links", args[2].get())?.unwrap_or_default(),
			Arguments::check_optional("include_dirs", args[3].get())?.unwrap_or_default(),
			Arguments::check_optional("defines", args[4].get())?.unwrap_or_default(),
			Arguments::check_optional("link_flags", args[5].get())?.unwrap_or_default(),
		)?;
		Ok(eval.heap().alloc(v))
	}
}

pub(crate) fn build_api(project: &Arc<Mutex<StarProject>>, builder: &mut GlobalsBuilder) {
	{
		let mut sig_builder = starlark::eval::ParametersSpec::new("add_library".to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("link_private");
		sig_builder.optional("include_dirs_public");
		sig_builder.optional("include_dirs_private");
		sig_builder.optional("defines_public");
		sig_builder.optional("link_flags_public");
		let signature = sig_builder.finish();
		let documentation = {
			let parameter_types = HashMap::from([
				(0, DocType { raw_type: <&str>::starlark_type_repr() }),
				(1, DocType { raw_type: <Vec<&str>>::starlark_type_repr() }),
				(2, DocType { raw_type: <&str>::starlark_type_repr() }),
				(3, DocType { raw_type: <Value>::starlark_type_repr() }),
				(4, DocType { raw_type: <Vec<&str>>::starlark_type_repr() }),
				(5, DocType { raw_type: <Vec<&str>>::starlark_type_repr() }),
				(6, DocType { raw_type: <Vec<&str>>::starlark_type_repr() }),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: Some(DocType { raw_type: <Value>::starlark_type_repr() }),
				dot_type: None,
			}
		};
		builder.set_function(
			"add_library",
			false,
			documentation,
			None,
			ImplAddLibrary {
				signature,
				project: project.clone(),
			},
		);
	}
	{
		let mut sig_builder = starlark::eval::ParametersSpec::new("add_executable".to_owned());
		sig_builder.no_more_positional_only_args();
		sig_builder.required("name");
		sig_builder.required("sources");
		sig_builder.optional("links");
		sig_builder.optional("include_dirs");
		sig_builder.optional("defines");
		sig_builder.optional("link_flags");
		let signature = sig_builder.finish();

		let documentation = {
			let parameter_types = HashMap::from([
				(0, DocType { raw_type: <&str>::starlark_type_repr() }),
				(1, DocType { raw_type: <Vec<&str>>::starlark_type_repr() }),
				(2, DocType { raw_type: <Value>::starlark_type_repr() }),
				(3, DocType { raw_type: <Vec<String>>::starlark_type_repr() }),
				(4, DocType { raw_type: <Vec<String>>::starlark_type_repr() }),
				(5, DocType { raw_type: <Vec<String>>::starlark_type_repr() }),
			]);
			starlark::values::function::NativeCallableRawDocs {
				rust_docstring: None,
				signature: signature.clone(),
				parameter_types,
				return_type: Some(DocType { raw_type: <Value>::starlark_type_repr() }),
				dot_type: None,
			}
		};

		builder.set_function(
			"add_executable",
			false,
			documentation,
			None,
			ImplAddExecutable { signature, project: project.clone() },
		);
	}
}
