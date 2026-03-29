mod commands;

use std::{env, process::ExitCode};

use clap::{Arg, Command};

use crate::commands::{generate::handle_generate_command, new::handle_new_command};

use catapult::{
	// commands::new::handle_new_command, //
	generator::Generator,
	toolchain,
};

const SOURCE_DIR: &str = "source-dir";
const BUILD_DIR: &str = "build-dir";
const GENERATOR: &str = "generator";
const TOOLCHAIN: &str = "toolchain";
const PROFILE: &str = "profile";
const PACKAGE_OPTION: &str = "package-option";

fn main() -> ExitCode {
	env_logger::Builder::from_env(env_logger::Env::default().filter_or("CATAPULT_LOG", "off"))
		.format_timestamp(None)
		.init();

	let cli_command = build_cli();
	match cli_command.get_matches().subcommand() {
		Some(("new", sub_matches)) => {
			if handle_new_command(sub_matches).is_err() {
				return ExitCode::FAILURE;
			}
		}
		Some(("generate", sub_matches)) => return handle_generate_command(sub_matches),

		_ => {
			eprintln!("No subcommand was used");
			return ExitCode::FAILURE;
		}
	};
	ExitCode::SUCCESS
}

fn build_cli() -> Command {
	Command::new("catapult")
		.about("A package manager + build system for C and C++")
		.version(env!("CARGO_PKG_VERSION"))
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("new")
				.about("Create a new catapult project")
				.arg(Arg::new("name").help("Project name").required(true).index(1))
				.arg(
					Arg::new("bin")
						.long("bin")
						.help("Create a binary project (default)")
						.action(clap::ArgAction::SetTrue),
				)
				.arg(
					Arg::new("lib")
						.long("lib")
						.help("Create a library project")
						.action(clap::ArgAction::SetTrue),
				)
				.arg(
					Arg::new("path")
						.long("path")
						.value_name("<path-to-project>")
						.help("Directory to create the project in"),
				),
		)
		.subcommand(
			Command::new("generate")
				.about("Generate build files for the project")
				.arg(
					Arg::new(SOURCE_DIR)
						.short('S')
						.long(SOURCE_DIR)
						.value_name("<path-to-source>")
						.help("Specify the source directory")
						.required(true),
				)
				.arg(
					Arg::new(BUILD_DIR)
						.short('B')
						.long(BUILD_DIR)
						.value_name("<path-to-build>")
						.help("Specify the build directory")
						.required(true),
				)
				.arg(
					Arg::new(GENERATOR)
						.short('G')
						.long(GENERATOR)
						.value_name("<generator-name>")
						.help("Specify a build system generator")
						.required(true),
				)
				.arg(
					Arg::new(TOOLCHAIN)
						.short('T')
						.long(TOOLCHAIN)
						.value_name("<path-to-toolchain-file>")
						.help("Specify a path to a toolchain file"),
				)
				.arg(
					Arg::new(PROFILE)
						.short('P')
						.long(PROFILE)
						.value_name("<profile-name>")
						.help("Specify the profile to build"),
				)
				.arg(
					Arg::new(PACKAGE_OPTION)
						.short('p')
						.long(PACKAGE_OPTION)
						.value_name("<package-name>:<package-option>=<value>")
						.help("Override a package option")
						.action(clap::ArgAction::Append),
				),
		)
}
