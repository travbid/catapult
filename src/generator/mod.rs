mod compiler;
mod msvc;
mod ninja;

use std::{
	fs,
	path::Path, //
	sync::Arc,
};

use serde::Deserialize;

use crate::{project::Project, GlobalOptions};
use compiler::{identify_compiler, Compiler};

#[derive(Debug, Deserialize)]
pub struct ToolchainFile {
	c_compiler: Option<Vec<String>>,
	cpp_compiler: Option<Vec<String>>,
	static_linker: Option<Vec<String>>,
	exe_linker: Option<Vec<String>>,
	// env: Option<HashMap<String, String>>
}

pub enum Generator {
	Msvc,
	Ninja,
}

impl Generator {
	pub fn read_toolchain(toolchain_path: &Path) -> Result<ToolchainFile, String> {
		let toolchain_toml = match fs::read_to_string(toolchain_path) {
			Ok(x) => x,
			Err(e) => return Err(format!("Error opening toolchain file \"{}\": {}", toolchain_path.display(), e)),
		};

		let toolchain = match toml::from_str::<ToolchainFile>(&toolchain_toml) {
			Ok(x) => x,
			Err(e) => return Err(format!("Error reading toolchain file \"{}\": {}", toolchain_path.display(), e)),
		};

		Ok(toolchain)
	}

	pub fn generate(
		&self,
		project: Arc<Project>,
		global_opts: GlobalOptions,
		build_dir: &Path,
		toolchain: ToolchainFile,
	) -> Result<(), String> {
		match self {
			Generator::Msvc => msvc::Msvc::generate(project, build_dir, global_opts),
			Generator::Ninja => {
				let c_compiler = match toolchain.c_compiler {
					Some(x) => match identify_compiler(x) {
						Ok(y) => y,
						Err(e) => return Err(format!("Error identifying C compiler: {}", e)),
					},
					None => return Err("Toolchain file does not contain required field \"c_compiler\"".to_owned()),
				};
				let cpp_compiler = match toolchain.cpp_compiler {
					Some(x) => match identify_compiler(x) {
						Ok(y) => y,
						Err(e) => return Err(format!("Error identifying C++ compiler: {}", e)),
					},
					None => return Err("Toolchain file does not contain required field \"cpp_compiler\"".to_owned()),
				};
				let static_linker = match toolchain.static_linker {
					Some(x) => x,
					None => return Err("Toolchain file does not contain required field \"static_linker\"".to_owned()),
				};
				let exe_linker = match toolchain.exe_linker {
					Some(x) => x,
					None => return Err("Toolchain file doesn contain required field \"exe_linker\"".to_owned()),
				};

				let toolchain = Toolchain { c_compiler, cpp_compiler, static_linker, exe_linker };
				let target_platform = if cfg!(windows) {
					TargetPlatform {
						obj_ext: ".obj".to_owned(),
						static_lib_ext: ".lib".to_owned(),
						exe_ext: ".exe".to_owned(),
					}
				} else {
					TargetPlatform {
						obj_ext: ".o".to_owned(),
						static_lib_ext: ".a".to_owned(),
						exe_ext: "".to_owned(),
					}
				};
				ninja::Ninja::generate(project, build_dir, toolchain, global_opts, target_platform)
			}
		}
	}
}

pub trait StaticLinker {
	fn cmd(&self) -> Vec<String>;
}

pub trait ExeLinker {
	fn cmd(&self) -> Vec<String>;
}

pub struct Toolchain {
	pub c_compiler: Box<dyn Compiler>,
	pub cpp_compiler: Box<dyn Compiler>,
	pub static_linker: Vec<String>,
	pub exe_linker: Vec<String>,
}

pub struct TargetPlatform {
	pub obj_ext: String,
	pub static_lib_ext: String,
	pub exe_ext: String,
}
