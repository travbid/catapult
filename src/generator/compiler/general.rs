use super::Compiler;

pub(crate) struct GeneralCompiler {
	pub(super) cmd: Vec<String>,
}

impl Compiler for GeneralCompiler {
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
			_ => Err(format!("C++ standard not supported by compiler: {std}")),
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
}
