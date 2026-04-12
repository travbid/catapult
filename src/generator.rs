mod msvc;
mod ninja;
mod xcode;

use std::{
	path::Path, //
	sync::Arc,
};

use crate::{
	GlobalOptions,
	project::Project,
	toolchain::{Profile, Toolchain},
};

pub enum Generator {
	Msvc,
	Ninja,
	Xcode,
}

enum Os {
	Darwin,
	Linux,
	Windows,
}

impl Generator {
	pub fn generate(
		&self,
		project: Arc<Project>,
		global_opts: GlobalOptions,
		build_dir: &Path,
		toolchain: Toolchain,
		profile: Profile,
	) -> Result<(), String> {
		match self {
			Generator::Msvc => msvc::Msvc::generate(project, build_dir, toolchain, global_opts),
			Generator::Ninja => {
				let target_triple = if let Some(compiler) = &toolchain.c_compiler {
					compiler.target()
				} else if let Some(compiler) = &toolchain.cpp_compiler {
					compiler.target()
				} else {
					String::new()
				};
				log::info!("target_triple: {}", target_triple);
				let target_platform = TargetPlatform::from_target_triple(&target_triple);
				ninja::Ninja::generate(project, build_dir, toolchain, profile, global_opts, target_platform)
			}
			Generator::Xcode => xcode::Xcode::generate(project, build_dir, toolchain, global_opts),
		}
	}
}

pub struct TargetPlatform {
	os: Os,
	pub obj_ext: String,
	pub static_lib_ext: String,
	pub shared_lib_ext: String,
	pub shared_link_ext: String,
	pub exe_ext: String,
}

impl TargetPlatform {
	pub fn from_target_triple(target_triple: &str) -> Self {
		let target_platform = if target_triple.contains("-windows-") || target_triple.ends_with("-windows") {
			TargetPlatform {
				os: Os::Windows,
				obj_ext: ".obj".to_owned(),
				static_lib_ext: ".lib".to_owned(),
				shared_lib_ext: ".dll".to_owned(),
				shared_link_ext: ".lib".to_owned(),
				exe_ext: ".exe".to_owned(),
			}
		} else if target_triple.contains("-darwin-") || target_triple.ends_with("-darwin") {
			TargetPlatform {
				os: Os::Darwin,
				obj_ext: ".o".to_owned(),
				static_lib_ext: ".a".to_owned(),
				shared_lib_ext: ".dylib".to_owned(),
				shared_link_ext: ".dylib".to_owned(),
				exe_ext: "".to_owned(),
			}
		} else {
			TargetPlatform {
				os: Os::Linux,
				obj_ext: ".o".to_owned(),
				static_lib_ext: ".a".to_owned(),
				shared_lib_ext: ".so".to_owned(),
				shared_link_ext: ".so".to_owned(),
				exe_ext: "".to_owned(),
			}
		};
		target_platform
	}
	pub fn shared_runtime_identity_flags(&self, lib_name: &str) -> Option<String> {
		match self.os {
			Os::Darwin => Some(format!("-Wl,-install_name,@rpath/{lib_name}.dylib")),
			Os::Linux => Some(format!("-Wl,-soname,{lib_name}.so")),
			Os::Windows => None,
		}
	}

	pub fn runtime_search_path_flags(&self, paths: &[String]) -> Vec<String> {
		match self.os {
			Os::Windows => Vec::new(),
			Os::Darwin | Os::Linux => paths.iter().map(|path| format!("-Wl,-rpath,{path}")).collect(),
		}
	}
}
