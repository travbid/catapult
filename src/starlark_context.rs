use core::fmt;

use allocative::Allocative;
use starlark::{
	starlark_simple_value,
	values::{
		Heap, //
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

const PAD: &str = "";
const INDENT_SIZE: usize = 4;

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(crate) struct StarContext {
	pub c_compiler: Option<StarContextCompiler>,
	pub cpp_compiler: Option<StarContextCompiler>,
}

impl fmt::Display for StarContext {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		writeln!(f, "Context {{")?;
		write!(f, "{PAD:width_plus$}c_compiler: ")?;
		if let Some(compiler) = &self.c_compiler {
			writeln!(f, "{:width_plus$}", compiler)?;
		} else {
			writeln!(f, "None")?;
		}
		write!(f, "{PAD:width_plus$}cpp_compiler: ")?;
		if let Some(compiler) = &self.cpp_compiler {
			writeln!(f, "{:width_plus$}", compiler)?;
		} else {
			writeln!(f, "None")?;
		}
		write!(f, "{PAD:width$}}}")
	}
}

#[starlark::values::starlark_value(type = "Context")]
impl<'v> StarlarkValue<'v> for StarContext {
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"c_compiler" => Some(heap.alloc(self.c_compiler.clone())),
			"cpp_compiler" => Some(heap.alloc(self.cpp_compiler.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"c_compiler" | "cpp_compiler" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["c_compiler".to_owned(), "cpp_compiler".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarContext);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(crate) struct StarContextCompiler {
	pub target_triple: String,
}

impl fmt::Display for StarContextCompiler {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"ContextCompiler {{
{PAD:width_plus$}target_triple: {:width_plus$},
{PAD:width$}}}"#,
			self.target_triple,
		)
	}
}

#[starlark::values::starlark_value(type = "ContextCompiler")]
impl<'v> StarlarkValue<'v> for StarContextCompiler {
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"target_triple" => Some(heap.alloc(self.target_triple.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"target_triple" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["target_triple".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarContextCompiler);
