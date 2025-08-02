use std::{
	collections::BTreeMap, //
	env,
	fs,
	path,
	process::ExitCode,
};

use catapult::{generator::Generator, toolchain};

use clap::ArgMatches;

pub(crate) fn handle_generate_command(matches: &ArgMatches) -> ExitCode {
	let src_dir = matches.get_one::<String>("source-dir").unwrap();
	let build_dir = matches.get_one::<String>("build-dir").unwrap();
	let generator_str = matches.get_one::<String>("generator").unwrap();
	let toolchain_opt = matches.get_one::<String>("toolchain");
	let profile_opt = matches.get_one::<String>("profile");
	let package_opts_vec: Vec<&String> = matches
		.get_many::<String>("package-option")
		.unwrap_or_default()
		.collect();

	let toolchain_path = match toolchain_opt {
		Some(x) => path::PathBuf::from(x),
		None => {
			let cache_dir = match dirs::config_dir() {
				Some(x) => x,
				None => {
					println!("Could not find a config directory");
					return ExitCode::FAILURE;
				}
			};
			let tc_path = cache_dir.join("default_toolchain.toml");
			if !tc_path.exists() {
				// Create a default toolchain file if one doesn't already exist
				match fs::File::create(&tc_path) {
					Ok(_) => { // TODO(Travers)
					}
					Err(e) => {
						println!("Could not create a default toolchain file: {}", e);
						return ExitCode::FAILURE;
					}
				};
			}
			tc_path
		}
	};

	type InnerMap = BTreeMap<String, String>;
	let mut package_options = BTreeMap::<String, InnerMap>::new();
	for pkg_opt in package_opts_vec {
		let (pkg_name, opt) = match pkg_opt.split_once(':') {
			Some(x) => x,
			None => {
				println!("Invalid package-option. Option must be specified as <package name>:<package option>=<value>");
				return ExitCode::FAILURE;
			}
		};
		let (opt_name, opt_val) = match opt.split_once('=') {
			Some(x) => x,
			None => {
				println!("Invalid package-option. Option must be specified as <package name>:<package option>=<value>");
				return ExitCode::FAILURE;
			}
		};
		if let Some(map) = package_options.get_mut(pkg_name) {
			map.insert(opt_name.to_owned(), opt_val.to_owned());
		} else {
			let mut inner_map = InnerMap::new();
			inner_map.insert(opt_name.to_owned(), opt_val.to_owned());
			package_options.insert(pkg_name.to_owned(), inner_map);
		}
	}

	println!("     source-dir: {}", src_dir);
	println!("      build-dir: {}", build_dir);
	println!("      generator: {}", generator_str);
	println!("      toolchain: {}", toolchain_path.display());
	println!("        profile: {}", profile_opt.unwrap_or(&String::new()));
	println!("package-options: {}", {
		let mut ret = String::new();
		for (pkg_name, opts) in &package_options {
			for (opt_name, opt_val) in opts {
				ret += &format!("{pkg_name}:{opt_name}={opt_val} ");
			}
		}
		ret.pop();
		ret
	});

	let generator = match generator_str.as_str() {
		"Ninja" => Generator::Ninja,
		"MSVC" => Generator::Msvc,
		"Xcode" => Generator::Xcode,
		gen => {
			println!("Error: Not a valid generator '{}'", gen);
			return ExitCode::FAILURE;
		}
	};

	let original_dir = match env::current_dir() {
		Ok(x) => x,
		Err(e) => {
			println!("Error getting cwd: {}", e);
			return ExitCode::FAILURE;
		}
	};

	// Check source dir exists before erroring on anything else
	match env::set_current_dir(src_dir) {
		Ok(x) => x,
		Err(e) => {
			println!("Error setting cwd: {} (path: {})", e, src_dir);
			return ExitCode::FAILURE;
		}
	};

	// Check build dir can be created before erroring on anything else
	let build_dir_path = original_dir.join(build_dir);
	match fs::create_dir_all(&build_dir_path) {
		Ok(x) => x,
		Err(e) => {
			println!("Error creating directory: {} (path: {})", e, build_dir_path.display());
			return ExitCode::FAILURE;
		}
	}

	// Check toolchain before fetching dependencies
	let toolchain_path = if toolchain_path.is_absolute() {
		toolchain_path
	} else {
		original_dir.join(toolchain_path)
	};
	let toolchain = match toolchain::get_toolchain(&toolchain_path, matches!(generator, Generator::Msvc)) {
		Ok(x) => x,
		Err(e) => {
			println!("Toolchain error: {}", e);
			return ExitCode::FAILURE;
		}
	};

	// Check selected profile is provided by toolchain
	let profile = if let Some(prof) = profile_opt {
		if let Generator::Msvc = generator {
			println!("--profile is incompatible with MSVC generator");
			return ExitCode::FAILURE;
		};
		match toolchain.profile.get(prof) {
			None => {
				println!("Selected profile is not provided by toolchain");
				return ExitCode::FAILURE;
			}
			Some(x) => x.clone(),
		}
	} else {
		Default::default()
	};

	let (project, global_opts) = match catapult::parse_project(&toolchain, package_options) {
		Ok(x) => x,
		Err(e) => {
			println!("{}", e);
			return ExitCode::FAILURE;
		}
	};

	match generator.generate(project, global_opts, &build_dir_path, toolchain, profile) {
		Ok(x) => x,
		Err(e) => {
			println!("{}", e);
			return ExitCode::FAILURE;
		}
	};

	ExitCode::SUCCESS
}
