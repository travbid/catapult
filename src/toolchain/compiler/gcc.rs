use super::{Compiler, ExeLinker};

pub(crate) struct Gcc {
	pub(super) cmd: Vec<String>,
	#[allow(dead_code)]
	pub(super) version: String,
	#[allow(dead_code)]
	pub(super) target: String,
}

impl Compiler for Gcc {
	fn cmd(&self) -> Vec<String> {
		self.cmd.clone()
	}

	fn out_flag(&self) -> String {
		"-o".to_owned()
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
		Some("-fPIC".to_owned())
	}

	fn position_independent_executable_flag(&self) -> Option<String> {
		Some("-fPIE".to_owned())
	}
}

impl ExeLinker for Gcc {
	fn cmd(&self) -> Vec<String> {
		self.cmd.clone()
	}

	fn position_independent_executable_flag(&self) -> Option<String> {
		Some("-pie".to_owned())
	}
}
