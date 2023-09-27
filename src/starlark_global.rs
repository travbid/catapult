use core::fmt;
use std::collections::HashMap;

use allocative::Allocative;
use serde::Deserialize;
use starlark::{
	starlark_simple_value, starlark_type,
	values::{
		AllocValue,
		Heap, //
		NoSerialize,
		ProvidesStaticType,
		StarlarkValue,
		Value,
	},
};

use super::GlobalOptions;
use crate::toolchain::Toolchain;

#[derive(Clone, Debug, Allocative)]
pub enum PkgOpt {
	Bool(bool),
	Int(i64),
	Float(f64),
	String(String),
}

impl fmt::Display for PkgOpt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		match self {
			PkgOpt::Bool(b) => write!(f, "{}", b),
			PkgOpt::Int(i) => write!(f, "{}", i),
			PkgOpt::Float(x) => write!(f, "{}", x),
			PkgOpt::String(s) => write!(f, "{}", s),
		}
	}
}

impl<'de> Deserialize<'de> for PkgOpt {
	fn deserialize<D>(d: D) -> Result<Self, <D as serde::Deserializer<'de>>::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct PkgOptVisitor;

		impl<'de> serde::de::Visitor<'de> for PkgOptVisitor {
			type Value = PkgOpt;

			fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
				write!(f, "bool|int|float|str")
			}

			fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				Ok(PkgOpt::Bool(v))
			}

			fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				Ok(PkgOpt::Int(v))
			}

			fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				Ok(PkgOpt::Float(v))
			}

			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				Ok(PkgOpt::String(v.to_owned()))
			}

			fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				Ok(PkgOpt::String(v))
			}
		}
		d.deserialize_any(PkgOptVisitor)
	}
}

#[derive(Debug, Allocative, ProvidesStaticType, NoSerialize)]
pub(super) struct StarGlobal {
	global_options: StarGlobalOptions,
	package_options: StarPackageOptions,
	toolchain: StarToolchain,
}

impl StarGlobal {
	pub(super) fn new(
		options: &GlobalOptions,
		package_options: HashMap<String, PkgOpt>,
		toolchain: &Toolchain,
	) -> StarGlobal {
		let c_compiler = toolchain.c_compiler.as_ref().map(|compiler| StarCompiler {
			id: compiler.id(),
			version: StarVersion::from_str(compiler.version()),
		});
		let cpp_compiler = toolchain.cpp_compiler.as_ref().map(|compiler| StarCompiler {
			id: compiler.id(),
			version: StarVersion::from_str(compiler.version()),
		});
		StarGlobal {
			global_options: StarGlobalOptions {
				c_standard: options.c_standard.clone(),
				cpp_standard: options.cpp_standard.clone(),
				position_independent_code: options.position_independent_code,
			},
			package_options: StarPackageOptions(package_options),
			toolchain: StarToolchain { c_compiler, cpp_compiler },
		}
	}
}

impl fmt::Display for StarGlobal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Global{{
    global_options: {},
    package_options: {},
    toolchain: {},
}}"#,
			self.global_options, self.package_options, self.toolchain,
		)
	}
}

impl<'v> StarlarkValue<'v> for StarGlobal {
	starlark_type!("Global");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"global_options" => Some(heap.alloc(self.global_options.clone())),
			"package_options" => Some(heap.alloc(self.package_options.clone())),
			"toolchain" => Some(heap.alloc(self.toolchain.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"global_options" | "package_options" | "toolchain" => true,
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
pub(super) struct StarGlobalOptions {
	c_standard: Option<String>,
	cpp_standard: Option<String>,
	position_independent_code: Option<bool>,
}

impl fmt::Display for StarGlobalOptions {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"GlobalOptions{{
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

impl<'v> StarlarkValue<'v> for StarGlobalOptions {
	starlark_type!("GlobalOptions");

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

starlark_simple_value!(StarGlobalOptions);

#[derive(Clone, Debug, Allocative, ProvidesStaticType, NoSerialize)]
struct StarPackageOptions(HashMap<String, PkgOpt>);

impl fmt::Display for StarPackageOptions {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "PackageOptions{{")?;
		for (key, val) in &self.0 {
			writeln!(f, "   {}: {}", key, val)?;
		}
		writeln!(f, "}}")
	}
}

impl<'v> StarlarkValue<'v> for StarPackageOptions {
	starlark_type!("PackageOptions");

	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match self.0.get(attribute) {
			None => None,
			Some(x) => match x {
				PkgOpt::Bool(b) => Some(Value::new_bool(*b)),
				PkgOpt::Int(i) => Some(i.alloc_value(heap)),
				PkgOpt::Float(f) => Some(f.alloc_value(heap)),
				PkgOpt::String(s) => Some(s.alloc_value(heap)),
			},
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		self.0.contains_key(attribute)
	}

	fn dir_attr(&self) -> Vec<String> {
		self.0.keys().cloned().collect()
	}
}

starlark_simple_value!(StarPackageOptions);

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
