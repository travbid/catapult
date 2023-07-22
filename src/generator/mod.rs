mod msvc;
mod ninja;

use crate::project::Project;
use std::{
	path::PathBuf, //
	sync::Arc,
};

pub enum Generator {
	Msvc,
	Ninja,
}

impl Generator {
	pub fn generate(&self, project: Arc<Project>, build_dir: PathBuf) -> Result<(), String> {
		match self {
			Generator::Msvc => msvc::Msvc::generate(project, build_dir),
			Generator::Ninja => {
				let build_tools = BuildTools {
					c_compiler: vec!["clang".to_owned()],
					cpp_compiler: vec!["clang++".to_owned()],
					static_linker: vec!["llvm-ar".to_owned(), "qc".to_owned()],
					exe_linker: vec!["clang++".to_owned()],
					out_flag: "-o".to_owned(),
				};
				let compile_options = Vec::new(); // TODO(Travers)
				let target_platform = TargetPlatform {
					obj_ext: ".o".to_owned(),
					static_lib_ext: ".a".to_owned(),
					exe_ext: "".to_owned(),
				};
				ninja::Ninja::generate(project, build_dir, build_tools, compile_options, target_platform)
			}
		}
	}
}

pub trait Compiler {
	fn cmd(&self) -> String;
	fn compile_flags(&self) -> Vec<String>;
	fn compile_object_out_flags(&self, out_file: &str) -> Vec<String>;
}

pub trait StaticLinker {
	fn cmd(&self) -> Vec<String>;
}

pub trait ExeLinker {
	fn cmd(&self) -> Vec<String>;
}

pub struct BuildTools {
	pub c_compiler: Vec<String>,
	pub cpp_compiler: Vec<String>,
	pub static_linker: Vec<String>,
	pub exe_linker: Vec<String>,

	pub out_flag: String,
}

pub struct TargetPlatform {
	pub obj_ext: String,
	pub static_lib_ext: String,
	pub exe_ext: String,
}
