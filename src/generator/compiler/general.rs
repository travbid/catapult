use super::Compiler;

pub(crate) struct GeneralCompiler {
	pub(super) cmd: Vec<String>,
}

impl Compiler for GeneralCompiler {
	fn cmd(&self) -> Vec<String> {
		self.cmd.clone()
	}

	fn out_flag(&self) -> String {
		return "-o".to_owned();
	}
}
