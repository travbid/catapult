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

	let catapult_toml = include_str!("templates/catapult.toml").replace("{name}", name);
	write_file("catapult.toml", &catapult_toml)?;
	if is_bin {
		let build = include_str!("templates/build_bin.catapult").replace("{name}", name);
		write_file("build.catapult", &build)?;
		write_file("src/main.cpp", include_str!("templates/main.cpp"))?;
	} else {
		let build = include_str!("templates/build_lib.catapult").replace("{name}", name);
		write_file("build.catapult", &build)?;
		write_file("src/include/lib.hpp", include_str!("templates/lib.hpp"))?;
		write_file("src/lib.cpp", include_str!("templates/lib.cpp"))?;
	}

	write_file(".clang-format", include_str!("templates/clang-format"))?;

	write_file(".gitignore", include_str!("templates/gitignore"))?;

	Ok(())
}
