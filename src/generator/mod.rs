mod msvc;
mod ninja;

use std::{
	path::Path, //
	sync::Arc,
};

use crate::{project::Project, toolchain::Toolchain, GlobalOptions};

pub enum Generator {
	Msvc,
	Ninja,
}

impl Generator {
	pub fn generate(
		&self,
		project: Arc<Project>,
		global_opts: GlobalOptions,
		build_dir: &Path,
		toolchain: Toolchain,
	) -> Result<(), String> {
		match self {
			Generator::Msvc => msvc::Msvc::generate(project, build_dir, global_opts),
			Generator::Ninja => {
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

pub struct TargetPlatform {
	pub obj_ext: String,
	pub static_lib_ext: String,
	pub exe_ext: String,
}
