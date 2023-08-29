mod general;

// use std::process;

use log;

pub trait Compiler {
	fn cmd(&self) -> Vec<String>;
	fn out_flag(&self) -> String;
	fn c_std_flag(&self, std: &str) -> Result<String, String>;
	fn cpp_std_flag(&self, std: &str) -> Result<String, String>;
}

pub(super) fn identify_compiler(cmd: Vec<String>) -> Result<Box<dyn Compiler>, String> {
	log::debug!("identify_compiler() cmd: {}", cmd.join(" "));
	Ok(Box::new(general::GeneralCompiler { cmd }))
	// let exe = match cmd.first() {
	// 	Some(x) => x,
	// 	None => return Err("Compiler command is empty".to_owned()),
	// };
	// let version_output = match process::Command::new(exe).arg("--version").output() {
	// 	Ok(x) => String::from_utf8_lossy(&x.stdout).into_owned(),
	// 	Err(e) => {
	// 		log::info!("Error executing compiler command \"{} --version\": {}", exe, e);
	// 		String::new()
	// 	}
	// };
	// if version_output.starts_with("clang ") || version_output.starts_with("Ubuntu clang ") {
	// 	return Ok(Box::new(clang::Clang { cmd }));
	// }
	// return Err(format!("Could not identify compiler \"{}\"", exe));
}
