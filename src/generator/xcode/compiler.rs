// This struct exists only so that recipe files that read e.g.
// `GLOBAL.toolchain.c_compiler` when used with the Xcode generator will get
// something reasonable instead of a compiler that won't actually be used.
pub(super) struct Xcode {}

impl crate::toolchain::compiler::Compiler for Xcode {
	fn id(&self) -> String {
		"Xcode".to_owned()
	}

	fn version(&self) -> String {
		// TODO(Travers): Currently the MSVC generator works even when Visual
		// Studio is not installed on the build machine. Eventually catapult will
		// need to query the VS installation for information such as the version.
		String::new()
	}

	fn target(&self) -> String {
		unimplemented!()
	}

	fn cmd(&self) -> Vec<String> {
		unimplemented!()
	}

	fn out_flag(&self) -> String {
		unimplemented!()
	}

	fn depfile_flags(&self, _out_file: &str, _dep_file: &str) -> Vec<String> {
		unimplemented!()
	}

	fn c_std_flag(&self, std: &str) -> Result<String, String> {
		match std {
			"11" => Ok("c11".to_owned()),
			"14" => Ok("c14".to_owned()),
			"17" => Ok("c17".to_owned()),
			"20" => Ok("c20".to_owned()),
			"23" => Ok("c23".to_owned()),
			_ => Err(format!("C standard not supported by compiler: {std}")),
		}
	}

	fn cpp_std_flag(&self, std: &str) -> Result<String, String> {
		match std {
			"11" => Ok("c++11".to_owned()),
			"14" => Ok("c++14".to_owned()),
			"17" => Ok("c++17".to_owned()),
			"20" => Ok("c++20".to_owned()),
			"23" => Ok("c++23".to_owned()),
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
