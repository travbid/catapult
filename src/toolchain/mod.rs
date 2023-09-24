pub(crate) mod compiler;

use std::{fs, path::Path};

use serde::Deserialize;

use compiler::{identify_compiler, identify_linker, Compiler, ExeLinker};

#[derive(Debug, Deserialize)]
pub struct ToolchainFile {
	c_compiler: Option<Vec<String>>,
	cpp_compiler: Option<Vec<String>>,
	static_linker: Option<Vec<String>>,
	exe_linker: Option<Vec<String>>,
	// env: Option<HashMap<String, String>>
}

#[derive(Default)]
pub struct Toolchain {
	pub c_compiler: Option<Box<dyn Compiler>>,
	pub cpp_compiler: Option<Box<dyn Compiler>>,
	pub static_linker: Option<Vec<String>>,
	pub exe_linker: Option<Box<dyn ExeLinker>>,
}

pub fn read_toolchain(toolchain_path: &Path) -> Result<Toolchain, String> {
	let toolchain_toml = match fs::read_to_string(toolchain_path) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error opening toolchain file \"{}\": {}", toolchain_path.display(), e)),
	};

	let toolchain_file = match toml::from_str::<ToolchainFile>(&toolchain_toml) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error reading toolchain file \"{}\": {}", toolchain_path.display(), e)),
	};

	let c_compiler = match toolchain_file.c_compiler {
		Some(x) => match identify_compiler(x) {
			Ok(y) => Some(y),
			Err(e) => return Err(format!("Error identifying C compiler: {}", e)),
		},
		None => None,
	};
	let cpp_compiler = match toolchain_file.cpp_compiler {
		Some(x) => match identify_compiler(x) {
			Ok(y) => Some(y),
			Err(e) => return Err(format!("Error identifying C++ compiler: {}", e)),
		},
		None => None,
	};
	let static_linker = toolchain_file.static_linker;

	let exe_linker = match toolchain_file.exe_linker {
		Some(x) => match identify_linker(x) {
			Ok(linker) => Some(linker),
			Err(e) => return Err(format!("Error identifying C++ compiler: {}", e)),
		},
		None => None,
	};

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

	let toolchain = Toolchain { c_compiler, cpp_compiler, static_linker, exe_linker };

	Ok(toolchain)
}
