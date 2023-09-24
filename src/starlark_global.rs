use core::fmt;

use allocative::Allocative;
use starlark::{
	starlark_simple_value, starlark_type,
	values::{
		Heap, //
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

use super::GlobalOptions;
use crate::toolchain::Toolchain;

#[derive(Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarGlobal {
	options: StarOptions,
	toolchain: StarToolchain,
}

impl StarGlobal {
	pub(super) fn new(options: &GlobalOptions, toolchain: &Toolchain) -> StarGlobal {
		let c_compiler = toolchain.c_compiler.as_ref().map(|compiler| StarCompiler {
			id: compiler.id(),
			version: StarVersion::from_str(compiler.version()),
		});
		let cpp_compiler = toolchain.cpp_compiler.as_ref().map(|compiler| StarCompiler {
			id: compiler.id(),
			version: StarVersion::from_str(compiler.version()),
		});
		StarGlobal {
			options: StarOptions {
				c_standard: options.c_standard.clone(),
				cpp_standard: options.cpp_standard.clone(),
				position_independent_code: options.position_independent_code,
			},
			toolchain: StarToolchain { c_compiler, cpp_compiler },
		}
	}
}

impl fmt::Display for StarGlobal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Global{{
    options: {},
}}"#,
			self.options,
		)
	}
}

impl<'v> StarlarkValue<'v> for StarGlobal {
	starlark_type!("Global");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"options" => Some(heap.alloc(self.options.clone())),
			"toolchain" => Some(heap.alloc(self.toolchain.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"options" | "toolchain" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["options".to_owned(), "toolchain".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarGlobal);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarOptions {
	c_standard: Option<String>,
	cpp_standard: Option<String>,
	position_independent_code: Option<bool>,
}

impl fmt::Display for StarOptions {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Options{{
    c_standard: {},
    cpp_standard: {},
    position_independent_code: {},
}}"#,
			self.c_standard.as_deref().unwrap_or("None"),
			self.cpp_standard.as_deref().unwrap_or("None"),
			self.position_independent_code
				.map(|x| x.to_string())
				.unwrap_or("None".to_owned())
		)
	}
}

impl<'v> StarlarkValue<'v> for StarOptions {
	starlark_type!("Options");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"c_standard" => Some(heap.alloc(self.c_standard.clone())),
			"cpp_standard" => Some(heap.alloc(self.cpp_standard.clone())),
			"position_independent_code" => Some(heap.alloc(self.position_independent_code)),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"c_standard" | "cpp_standard" | "position_independent_code" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec![
			"c_standard".to_owned(),
			"cpp_standard".to_owned(),
			"position_independent_code".to_owned(),
		];
		attrs
	}
}

starlark_simple_value!(StarOptions);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarToolchain {
	c_compiler: Option<StarCompiler>,
	cpp_compiler: Option<StarCompiler>,
}

impl fmt::Display for StarToolchain {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Toolchain{{
    c_compiler: {},
    cpp_compiler: {},
}}"#,
			self.c_compiler.as_ref().map_or("None".to_owned(), |x| x.to_string()),
			self.cpp_compiler.as_ref().map_or("None".to_owned(), |x| x.to_string()),
		)
	}
}

impl<'v> StarlarkValue<'v> for StarToolchain {
	starlark_type!("Toolchain");

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

starlark_simple_value!(StarToolchain);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarCompiler {
	id: String,
	version: StarVersion,
}

impl fmt::Display for StarCompiler {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Compiler{{
    id: "{}",
    version: {},
}}"#,
			self.id, self.version,
		)
	}
}

impl<'v> StarlarkValue<'v> for StarCompiler {
	starlark_type!("Compiler");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"id" => Some(heap.alloc(self.id.clone())),
			"version" => Some(heap.alloc(self.version.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"id" | "version" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["id".to_owned(), "version".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarCompiler);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarVersion {
	str: String,
	major: u32,
	minor: u32,
	patch: u32,
	revision: String,
}

impl StarVersion {
	fn from_str(ver: String) -> StarVersion {
		let str = ver.clone();
		let (semver, revision) = ver.split_once('-').unwrap_or((&ver, ""));
		let mut semver = semver.split('.');
		let major = semver.next().map_or(0, |x| x.parse().unwrap_or(0));
		let minor = semver.next().map_or(0, |x| x.parse().unwrap_or(0));
		let patch = semver.next().map_or(0, |x| x.parse().unwrap_or(0));
		StarVersion { str, major, minor, patch, revision: revision.to_owned() }
	}
}

impl fmt::Display for StarVersion {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Compiler{{
    str: "{}",
    major: {},
    minor: {},
    patch: {},
    revision: "{}",
}}"#,
			self.str, self.major, self.minor, self.patch, self.revision,
		)
	}
}

impl<'v> StarlarkValue<'v> for StarVersion {
	starlark_type!("Version");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"str" => Some(heap.alloc(self.str.clone())),
			"major" => Some(heap.alloc(self.major)),
			"minor" => Some(heap.alloc(self.minor)),
			"patch" => Some(heap.alloc(self.patch)),
			"revision" => Some(heap.alloc(self.revision.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"id" | "version" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec!["id".to_owned(), "version".to_owned()];
		attrs
	}
}

starlark_simple_value!(StarVersion);
