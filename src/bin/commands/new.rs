use std::{fs, path};

use clap::ArgMatches;

pub(crate) fn handle_new_command(matches: &ArgMatches) -> Result<(), ()> {
	let name = matches.get_one::<String>("name").unwrap();
	let is_bin = matches.get_flag("bin");
	let is_lib = matches.get_flag("lib");
	let project_path = matches.get_one::<String>("path");

	// Default to binary if neither bin nor lib is specified
	let project_type = if is_lib {
		"library"
	} else if is_bin {
		"binary"
	} else {
		eprintln!("Error: either --bin or --lib must be specified");
		return Err(());
	};

	println!("Creating new {project_type} project: {name}");

	let project_path = if let Some(path) = project_path {
		println!("Creating new {project_type} project \"{name}\" in: {path}");
		path::PathBuf::from(path)
	} else {
		println!("Creating new {project_type} project \"{name}\" in current directory");
		path::PathBuf::from(".").join(name)
	};

	if project_path.exists() {
		eprintln!("Error: Project path already exists: {}", project_path.display());
		return Err(());
	}

	match fs::create_dir_all(project_path.join("src")) {
		Ok(_) => {}
		Err(e) => {
			eprintln!("Error creating project directory: {}", e);
			return Err(());
		}
	}
	if is_lib {
		match fs::create_dir_all(project_path.join("src/include")) {
			Ok(_) => {}
			Err(e) => {
				eprintln!("Error creating project directory: {}", e);
				return Err(());
			}
		}
	}

	let write_file = |file_name: &str, content: &str| -> Result<(), ()> {
		match fs::write(project_path.join(file_name), content) {
			Ok(_) => Ok(()),
			Err(e) => {
				eprintln!("Error writing {}: {}", file_name, e);
				Err(())
			}
		}
	};

	write_file("catapult.toml", &format_catapult_toml(name))?;
	if is_bin {
		write_file("build.catapult", &format_build_catapult_bin(name))?;
		write_file("src/main.cpp", MAIN_CPP)?;
	} else {
		write_file("build.catapult", &format_build_catapult_lib(name))?;
		write_file("src/include/lib.hpp", LIB_HPP)?;
		write_file("src/lib.cpp", LIB_CPP)?;
	}
	write_file(".gitignore", GITIGNORE)?;

	Ok(())
}

fn format_catapult_toml(name: &str) -> String {
	format!(
		r#"[package]
name = "{}"

[dependencies]

[options]
c_standard = "23"
cpp_standard = "23"
position_independent_code = true
"#,
		name
	)
}

fn format_build_catapult_bin(name: &str) -> String {
	format!(
		r#"add_executable(
    name = "{}",
    sources = ["main.cpp"],
)
"#,
		name
	)
}

fn format_build_catapult_lib(name: &str) -> String {
	format!(
		r#"add_static_library(
    name = "{}",
    sources = ["lib.cpp"],
    include_dirs_public = ["."],
)
"#,
		name
	)
}

const MAIN_CPP: &str = r#"#include <print>

int main() {
    std::println("Hello, world!");
}
"#;

const LIB_CPP: &str = r#"#include "lib.hpp"

int add(int a, int b) {
    return a + b;
}
"#;

const LIB_HPP: &str = r#"#pragma once

int add(int a, int b);
"#;

const GITIGNORE: &str = r#"# Build artifacts
*.a
*.d
*.dll
*.exe
*.ilk
*.lib
*.log
*.o
*.out
*.pdb
*.so

# IDE directories
.idea/
.vscode/

.DS_Store
"#;
