use core::fmt;
use std::collections::HashMap;

use allocative::Allocative;
use serde::Deserialize;
use starlark::{
	starlark_simple_value,
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

const PAD: &str = "";
const INDENT_SIZE: usize = 4;

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
		let nasm_assembler = toolchain.nasm_assembler.as_ref().map(|assembler| StarAssembler {
			id: assembler.id(),
			version: StarVersion::from_str(assembler.version()),
		});
		StarGlobal {
			global_options: StarGlobalOptions {
				c_standard: options.c_standard.clone(),
				cpp_standard: options.cpp_standard.clone(),
				position_independent_code: options.position_independent_code,
			},
			package_options: StarPackageOptions(package_options),
			toolchain: StarToolchain { c_compiler, cpp_compiler, nasm_assembler },
		}
	}
}

impl fmt::Display for StarGlobal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"Global {{
{PAD:width_plus$}global_options: {:width_plus$},
{PAD:width_plus$}package_options: {:width_plus$},
{PAD:width_plus$}toolchain: {:width_plus$},
{PAD:width$}}}"#,
			self.global_options, self.package_options, self.toolchain,
		)
	}
}

#[starlark::values::starlark_value(type = "Global")]
impl<'v> StarlarkValue<'v> for StarGlobal {
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
		let attrs = vec![
			"global_options".to_owned(),
			"package_options".to_owned(),
			"toolchain".to_owned(),
		];
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
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"GlobalOptions {{
{PAD:width_plus$}c_standard: {},
{PAD:width_plus$}cpp_standard: {},
{PAD:width_plus$}position_independent_code: {},
{PAD:width$}}}"#,
			self.c_standard.as_deref().unwrap_or("None"),
			self.cpp_standard.as_deref().unwrap_or("None"),
			self.position_independent_code
				.map(|x| x.to_string())
				.unwrap_or("None".to_owned())
		)
	}
}

#[starlark::values::starlark_value(type = "GlobalOptions")]
impl<'v> StarlarkValue<'v> for StarGlobalOptions {
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
		if self.0.is_empty() {
			return write!(f, "PackageOptions {{}}");
		}
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		writeln!(f, "PackageOptions {{")?;
		for (key, val) in &self.0 {
			writeln!(f, "{PAD:width_plus$}{}: {}", key, val)?;
		}
		write!(f, "{PAD:width$}}}")
	}
}

#[starlark::values::starlark_value(type = "PackageOptions")]
impl<'v> StarlarkValue<'v> for StarPackageOptions {
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
	nasm_assembler: Option<StarAssembler>,
}

impl fmt::Display for StarToolchain {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		writeln!(f, "Toolchain {{")?;
		writeln!(f, "{PAD:width_plus$}c_compiler: ")?;
		if let Some(compiler) = &self.c_compiler {
			writeln!(f, "{:width_plus$}", compiler)?;
		} else {
			writeln!(f, "None")?;
		}
		writeln!(f, "{PAD:width_plus$}cpp_compiler: ")?;
		if let Some(compiler) = &self.cpp_compiler {
			writeln!(f, "{:width_plus$}", compiler)?;
		} else {
			writeln!(f, "None")?;
		}
		writeln!(f, "{PAD:width_plus$}nasm_assembler: ")?;
		if let Some(assembler) = &self.nasm_assembler {
			writeln!(f, "{:width_plus$}", assembler)?;
		} else {
			writeln!(f, "None")?;
		}
		write!(f, "{PAD:width$}}}")
	}
}

#[starlark::values::starlark_value(type = "Toolchain")]
impl<'v> StarlarkValue<'v> for StarToolchain {
	fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
		match attribute {
			"c_compiler" => Some(heap.alloc(self.c_compiler.clone())),
			"cpp_compiler" => Some(heap.alloc(self.cpp_compiler.clone())),
			"nasm_assembler" => Some(heap.alloc(self.nasm_assembler.clone())),
			_ => None,
		}
	}

	fn has_attr(&self, attribute: &str, _: &'v Heap) -> bool {
		#[allow(clippy::match_like_matches_macro)]
		match attribute {
			"c_compiler" | "cpp_compiler" | "nasm_assembler" => true,
			_ => false,
		}
	}

	fn dir_attr(&self) -> Vec<String> {
		let attrs = vec![
			"c_compiler".to_owned(),
			"cpp_compiler".to_owned(),
			"nasm_assembler".to_owned(),
		];
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
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"Compiler {{
{PAD:width_plus$}id: "{}",
{PAD:width_plus$}version: {:width_plus$},
{PAD:width$}}}"#,
			self.id, self.version
		)
	}
}

#[starlark::values::starlark_value(type = "Compiler")]
impl<'v> StarlarkValue<'v> for StarCompiler {
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
pub(super) struct StarAssembler {
	id: String,
	version: StarVersion,
}

impl fmt::Display for StarAssembler {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"Assembler {{
{PAD:width_plus$}id: "{}",
{PAD:width_plus$}version: {:width_plus$},
{PAD:width$}}}"#,
			self.id, self.version
		)
	}
}

#[starlark::values::starlark_value(type = "Assembler")]
impl<'v> StarlarkValue<'v> for StarAssembler {
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

starlark_simple_value!(StarAssembler);

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
		let width = f.width().unwrap_or(0);
		let width_plus = width + INDENT_SIZE;
		write!(
			f,
			r#"Version {{
{PAD:width_plus$}str: "{}",
{PAD:width_plus$}major: {},
{PAD:width_plus$}minor: {},
{PAD:width_plus$}patch: {},
{PAD:width_plus$}revision: "{}",
{PAD:width$}}}"#,
			self.str, self.major, self.minor, self.patch, self.revision,
		)
	}
}

#[starlark::values::starlark_value(type = "Version")]
impl<'v> StarlarkValue<'v> for StarVersion {
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
