use std::{
	env, //
	fs,
	process::ExitCode,
};

use getopts::Options;

use catapult::generator::Generator;

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

	let mut opts = Options::new();
	opts.optopt("S", SOURCE_DIR, "Specify the source directory", "<path-to-source>");
	opts.optopt("B", BUILD_DIR, "Specify the build directory", "<path-to-build>");
	opts.optopt("G", GENERATOR, "Specify a build system generator", "<generator-name>");
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

	println!("source-dir: {}", src_dir);
	println!(" build-dir: {}", build_dir);
	println!(" generator: {}", generator_str);

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

	let (project, global_opts) = match catapult::parse_project() {
		Ok(x) => x,
		Err(e) => {
			println!("{}", e);
			return ExitCode::FAILURE;
		}
	};

	match generator.generate(project, global_opts, &build_dir_path) {
		Ok(x) => x,
		Err(e) => {
			println!("{}", e);
			return ExitCode::FAILURE;
		}
	};

	ExitCode::SUCCESS
}
