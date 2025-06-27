pub(crate) mod compiler;

use std::{collections::BTreeMap, fs, path::Path};

use serde::Deserialize;

use compiler::{
	identify_assembler, //
	identify_compiler,
	identify_linker,
	Assembler,
	Compiler,
	ExeLinker,
};

#[derive(Debug, Deserialize)]
pub struct ToolchainFile {
	msvc_platforms: Option<Vec<String>>,
	xcode_platforms: Option<Vec<String>>,
	c_compiler: Option<Vec<String>>,
	cpp_compiler: Option<Vec<String>>,
	nasm_assembler: Option<Vec<String>>,
	static_linker: Option<Vec<String>>,
	exe_linker: Option<Vec<String>>,
	profile: Option<BTreeMap<String, Profile>>,
	// env: Option<HashMap<String, String>>
}

#[derive(Default)]
pub struct Toolchain {
	pub msvc_platforms: Vec<String>,
	pub xcode_platforms: Vec<String>,
	pub c_compiler: Option<Box<dyn Compiler>>,
	pub cpp_compiler: Option<Box<dyn Compiler>>,
	pub nasm_assembler: Option<Box<dyn Assembler>>,
	pub static_linker: Option<Vec<String>>,
	pub exe_linker: Option<Box<dyn ExeLinker>>,
	pub profile: BTreeMap<String, Profile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Profile {
	#[serde(default)]
	pub c_compile_flags: Vec<String>,
	#[serde(default)]
	pub cpp_compile_flags: Vec<String>,
	#[serde(default)]
	pub nasm_assemble_flags: Vec<String>,
	pub vcxproj: Option<VcxprojProfile>,
	pub xcodeproj: Option<XcodeprojectProfile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct VcxprojProfile {
	pub preprocessor_definitions: Vec<String>,
	pub property_group: BTreeMap<String, String>,
	pub cl_compile: BTreeMap<String, String>,
	pub link: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XcodeprojectProfile {
	#[serde(rename = "NativeTarget")]
	pub native_target: BTreeMap<String, PbxItem>,
	#[serde(rename = "Project")]
	pub project: BTreeMap<String, PbxItem>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)] // https://serde.rs/enum-representations.html
pub enum PbxItem {
	String(String),
	Vec(Vec<String>),
}

pub fn get_toolchain(toolchain_path: &Path, for_msvc: bool) -> Result<Toolchain, String> {
	let toolchain_toml = match fs::read_to_string(toolchain_path) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error opening toolchain file \"{}\": {}", toolchain_path.display(), e)),
	};

	let toolchain_file = match toml::from_str::<ToolchainFile>(&toolchain_toml) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error reading toolchain file \"{}\": {}", toolchain_path.display(), e)),
	};

	let msvc_platforms = toolchain_file.msvc_platforms.unwrap_or_default();
	let invalid_platforms = msvc_platforms
		.iter()
		.filter(|platform| match platform as &str {
			"ARM" | "ARM64" | "Win32" | "x64" => false,
			_ => true,
		})
		.cloned()
		.collect::<Vec<String>>();
	if !invalid_platforms.is_empty() {
		return Err(format!(
			"Invalid msvc platform in toolchain: {}. Valid msvc platforms are ARM, ARM64, Win32, x64",
			invalid_platforms.join(", ")
		));
	}

	let xcode_platforms = toolchain_file.xcode_platforms.unwrap_or_default();
	let invalid_platforms = xcode_platforms
		.iter()
		.filter(|platform| match platform as &str {
			"arm64" | "x86_64" => false,
			_ => true,
		})
		.cloned()
		.collect::<Vec<String>>();
	if !invalid_platforms.is_empty() {
		return Err(format!(
			"Invalid xcode platform in toolchain: {}. Valid xcode platforms are arm64, x86_64",
			invalid_platforms.join(", ")
		));
	}

	let nasm_assembler = match toolchain_file.nasm_assembler {
		Some(x) => match identify_assembler(x) {
			Ok(y) => Some(y),
			Err(e) => return Err(format!("Error identifying NASM assembler: {}", e)),
		},
		None => None,
	};
	let c_compiler: Option<Box<dyn Compiler>> = if for_msvc {
		Some(compiler::msvc_compiler())
	} else {
		match toolchain_file.c_compiler {
			Some(x) => match identify_compiler(x) {
				Ok(y) => Some(y),
				Err(e) => return Err(format!("Error identifying C compiler: {}", e)),
			},
			None => None,
		}
	};
	let cpp_compiler: Option<Box<dyn Compiler>> = if for_msvc {
		Some(compiler::msvc_compiler())
	} else {
		match toolchain_file.cpp_compiler {
			Some(x) => match identify_compiler(x) {
				Ok(y) => Some(y),
				Err(e) => return Err(format!("Error identifying C++ compiler: {}", e)),
			},
			None => None,
		}
	};
	let static_linker = toolchain_file.static_linker;

	let exe_linker = match toolchain_file.exe_linker {
		Some(x) => match identify_linker(x) {
			Ok(linker) => Some(linker),
			Err(e) => return Err(format!("Error identifying linker: {}", e)),
		},
		None => None,
	};

	let profile = toolchain_file.profile.unwrap_or_default();

	// Sanity checks
	if let Some(ref c_compiler) = c_compiler {
		if c_compiler.position_independent_code_flag().is_none() {
			log::info!("position_idependent_code not supported by the specified C compiler");
		}
	}
	if let Some(ref cpp_compiler) = cpp_compiler {
		if cpp_compiler.position_independent_code_flag().is_none() {
			log::info!("position_idependent_code not supported by the specified C++ compiler");
		}
	}

	let toolchain = Toolchain {
		msvc_platforms,
		xcode_platforms,
		nasm_assembler,
		c_compiler,
		cpp_compiler,
		static_linker,
		exe_linker,
		profile,
	};

	Ok(toolchain)
}
