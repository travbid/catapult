use super::Compiler;

// This struct exists only so that recipe files that read e.g.
// `GLOBAL.toolchain.c_compiler` when used with the MSVC generator will get
// something reasonable instead of a compiler that won't actually be used.
pub(super) struct Msvc {}

impl Compiler for Msvc {
	fn id(&self) -> String {
		"MSVC".to_owned()
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

	fn c_std_flag(&self, _std: &str) -> Result<String, String> {
		unimplemented!()
	}

	fn cpp_std_flag(&self, _std: &str) -> Result<String, String> {
		unimplemented!()
	}

	fn position_independent_code_flag(&self) -> Option<String> {
		None
	}

	fn position_independent_executable_flag(&self) -> Option<String> {
		None
	}
}
