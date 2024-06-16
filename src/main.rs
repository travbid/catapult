use std::{
	collections::BTreeMap,
	env, //
	fs,
	path,
	process::ExitCode,
};

use getopts::Options;

use catapult::{generator::Generator, toolchain};

fn print_usage(program: &str, opts: Options) {
	let brief = format!("Usage: {} FILE [options]", program);
	print!("{}", opts.usage(&brief));
}

fn main() -> ExitCode {
	env_logger::Builder::from_env(env_logger::Env::default().filter_or("CATAPULT_LOG", "off"))
		.format_timestamp(None)
		.init();

	let args: Vec<String> = env::args().collect();
	let program = args[0].clone();

	const SOURCE_DIR: &str = "source-dir";
	const BUILD_DIR: &str = "build-dir";
	const GENERATOR: &str = "generator";
	const TOOLCHAIN: &str = "toolchain";
	const PROFILE: &str = "profile";
	const PACKAGE_OPTION: &str = "package-option";

	let mut opts = Options::new();
	opts.optopt("S", SOURCE_DIR, "Specify the source directory", "<path-to-source>");
	opts.optopt("B", BUILD_DIR, "Specify the build directory", "<path-to-build>");
	opts.optopt("G", GENERATOR, "Specify a build system generator", "<generator-name>");
	opts.optopt("T", TOOLCHAIN, "Specify a path to a toolchain file", "<path-to-toolchain-file>");
	opts.optopt("P", PROFILE, "Specify the profile to build", "<profile-name>");
	opts.optmulti("p", PACKAGE_OPTION, "Override a package option", "<package name>:<option>=<value>");
	opts.optflag("h", "help", "print this help menu");
	let matches = match opts.parse(&args[1..]) {
		Ok(m) => m,
		Err(f) => {
			println!("Error: {}", f);
			print_usage(&program, opts);
			return ExitCode::FAILURE;
		}
	};
	if matches.opt_present("h") {
		print_usage(&program, opts);
		return ExitCode::SUCCESS;
	}

	let mut all_required_opts_present = true;
	let mut match_str = |opt: &str| -> String {
		match matches.opt_str(opt) {
			Some(x) => x,
			None => {
				println!("Error: Required option '--{}' missing", opt);
				all_required_opts_present = false;
				String::new()
			}
		}
	};
	let src_dir = match_str(SOURCE_DIR);
	let build_dir = match_str(BUILD_DIR);
	let generator_str = match_str(GENERATOR);
	if !all_required_opts_present {
		print_usage(&program, opts);
		return ExitCode::FAILURE;
	}

	let toolchain_path = match matches.opt_str(TOOLCHAIN) {
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

	let profile_opt = matches.opt_str(PROFILE);

	let package_opts_vec = matches.opt_strs(PACKAGE_OPTION);
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
	println!("        profile: {}", profile_opt.as_deref().unwrap_or_default());
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
	match env::set_current_dir(&src_dir) {
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
	let toolchain = match toolchain::read_toolchain(&toolchain_path) {
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
		match toolchain.profile.get(&prof) {
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
