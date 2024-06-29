use super::Assembler;

pub(crate) struct Nasm {
	pub cmd: Vec<String>,
	pub version: String,
}

impl Assembler for Nasm {
	fn id(&self) -> String {
		"nasm".to_owned()
	}

	fn version(&self) -> String {
		self.version.clone()
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
			dep_file.to_owned(),
			"-MT".to_owned(),
			out_file.to_owned(),
		]
	}
}
