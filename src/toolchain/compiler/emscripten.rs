use super::{Compiler, ExeLinker};

pub(crate) struct Emscripten {
	pub(super) cmd: Vec<String>,
	pub(super) version: String,
	pub(super) target: String,
}

impl Compiler for Emscripten {
	fn id(&self) -> String {
		"emscripten".to_owned()
	}

	fn version(&self) -> String {
		self.version.clone()
	}

	fn target(&self) -> String {
		self.target.clone()
	}

	fn cmd(&self) -> Vec<String> {
		self.cmd.clone()
	}

	fn out_flag(&self) -> String {
		"-o".to_owned()
	}

	fn depfile_flags(&self, out_file: &str, dep_file: &str) -> Vec<String> {
		vec![
			"-MD".to_owned(),
			"-MT".to_owned(),
			out_file.to_owned(),
			"-MF".to_owned(),
			dep_file.to_owned(),
		]
	}

	fn c_std_flag(&self, std: &str) -> Result<String, String> {
		match std {
			"11" => Ok("-std=c11".to_owned()),
			"17" => Ok("-std=c17".to_owned()),
			_ => Err(format!("C standard not supported by compiler: {std}")),
		}
	}

	fn cpp_std_flag(&self, std: &str) -> Result<String, String> {
		match std {
			"11" => Ok("-std=c++11".to_owned()),
			"14" => Ok("-std=c++14".to_owned()),
			"17" => Ok("-std=c++17".to_owned()),
			"20" => Ok("-std=c++20".to_owned()),
			"23" => Ok("-std=c++23".to_owned()),
			_ => Err(format!("C++ standard not supported by compiler: {std}")),
		}
	}

	fn position_independent_code_flag(&self) -> Option<String> {
		None
	}

	fn position_independent_executable_flag(&self) -> Option<String> {
		None
	}
}

impl ExeLinker for Emscripten {
	fn cmd(&self) -> Vec<String> {
		self.cmd.clone()
	}

	fn position_independent_executable_flag(&self) -> Option<String> {
		None
	}
}
