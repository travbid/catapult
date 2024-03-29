use core::default::Default;
use std::{
	collections::HashMap,
	io::Write,
	path::Path, //
	sync::Arc,
};

use log;

use super::{TargetPlatform, Toolchain};
use crate::{
	executable::Executable,
	link_type::LinkPtr,
	object_library::ObjectLibrary,
	project::Project,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
	toolchain::{
		compiler::{Compiler, ExeLinker},
		Profile,
	},
	GlobalOptions,
};

fn input_path(src: &Path, project_path: &Path) -> String {
	if src.is_relative() {
		project_path.join(src)
	} else {
		src.to_owned()
	}
	.to_str()
	.unwrap()
	.trim_start_matches(r"\\?\")
	.to_owned()
}

fn output_path(build_dir: &Path, project_name: &str, src: &str, ext: &str) -> String {
	build_dir
		.join(project_name)
		.join(src.to_owned() + ext)
		.to_str()
		.unwrap()
		.trim_start_matches(r"\\?\")
		.to_owned()
}

fn transform_defines(defines: &[String]) -> Vec<String> {
	defines
		.iter()
		.map(|x| {
			let mut s = x.split('=');
			let def_name = s.next().unwrap(); // MY_DEFINE
			let def_value = s.collect::<Vec<_>>();
			let def = if def_value.is_empty() {
				x.clone()
			} else {
				let def_value = def_value.join("=").replace('"', r#"\""#); // \"abc def\"
				if def_value.contains(char::is_whitespace) {
					def_name.to_owned() + r#"=""# + &def_value + r#"""# // MY_DEFINE="\"abc def\""
				} else {
					def_name.to_owned() + "=" + &def_value // MY_DEFINE=\"abcdef\"
				}
			};
			"-D".to_string() + &def
		})
		.collect()
}

#[derive(Clone)]
struct NinjaRspFile {
	rspfile: String,
	rspfilecontent: String,
}

#[derive(Clone, Default)]
struct NinjaRule {
	name: String,
	command: Vec<String>,
	depfile: Vec<String>,
	deps: Vec<String>,
	description: Option<String>,
	dyndep: Option<String>,
	generator: bool,
	restat: Option<String>,
	rspfile: Option<NinjaRspFile>,
}

impl NinjaRule {
	fn as_string(&self) -> String {
		let mut ret = format!(
			r#"rule {}
  command = {}"#,
			self.name,
			self.command.join(" ")
		);
		if !self.depfile.is_empty() {
			ret += "\n  depfile = ";
			ret += &self.depfile.join(" ");
		}
		if !self.deps.is_empty() {
			ret += "\n  deps = ";
			ret += &self.deps.join(" ");
		}
		if let Some(desc) = &self.description {
			ret += "\n  description = ";
			ret += desc;
		}
		if let Some(dyndep) = &self.dyndep {
			ret += "\n  dyndep = ";
			ret += dyndep;
		}
		if self.generator {
			ret += "\n  generator = 1";
		}
		if let Some(restat) = &self.restat {
			ret += "\n  restat = ";
			ret += restat;
		}
		if let Some(rspfile) = &self.rspfile {
			ret += "\n  rspfile = ";
			ret += &rspfile.rspfile;
			ret += "\n  rspfilecontent = ";
			ret += &rspfile.rspfilecontent;
		}
		ret += "\n\n";
		ret
	}
}

#[derive(Default)]
struct NinjaRules {
	compile_c_object: Option<NinjaRule>,
	compile_cpp_object: Option<NinjaRule>,
	link_static_lib: Option<NinjaRule>,
	link_exe: Option<NinjaRule>,
}

struct NinjaBuild {
	inputs: Vec<String>,
	output_targets: Vec<String>,
	rule: NinjaRule,
	keyval_set: HashMap<String, Vec<String>>,
}

impl NinjaBuild {
	fn as_string(&self) -> String {
		let mut ret = String::new();
		ret += &format!(
			"build {}: {} {}\n",
			self.output_targets.join(" ").replace(':', "$:"),
			self.rule.name,
			self.inputs.join(" ").replace(':', "$:"),
		);
		for (key, values) in &self.keyval_set {
			if !values.is_empty() {
				ret += &format!("  {key} = {}\n", values.join(" ").replace(':', "$:"));
			}
		}
		ret += "\n";
		ret
	}
}

fn compile_c_object(compiler: &dyn Compiler) -> NinjaRule {
	let mut command = compiler.cmd();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	// command.extend(compiler.compiler_flags(msvc_runtime));
	command.extend(vec![compiler.out_flag(), "$out".to_owned()]);
	command.extend(vec!["-c".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("compile_c_object"),
		command,
		description: Some("Compiling C object $out".to_owned()),
		..Default::default()
	}
}
fn compile_cpp_object(compiler: &dyn Compiler) -> NinjaRule {
	let mut command = compiler.cmd();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	command.extend(vec![compiler.out_flag(), "$out".to_owned()]);
	command.extend(vec!["-c".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("compile_cpp_object"),
		command,
		description: Some("Compiling C++ object $out".to_owned()),
		..Default::default()
	}
}
fn link_static_lib(static_linker: &[String]) -> NinjaRule {
	let mut command = static_linker.to_owned();
	command.extend(vec!["$TARGET_FILE".to_string(), "$LINK_FLAGS".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("link_static_lib"),
		command,
		description: Some("Linking static library $out".to_owned()),
		..Default::default()
	}
}
fn link_exe(exe_linker: &dyn ExeLinker) -> NinjaRule {
	let mut command = exe_linker.cmd();
	command.extend(vec![
		"$LINK_FLAGS".to_string(),
		"$in".to_string(),
		"-o".to_string(),
		"$TARGET_FILE".to_string(),
		"$LINK_PATH".to_string(),
	]);
	NinjaRule {
		name: String::from("link_exe"),
		command,
		description: Some("Linking executable $out".to_owned()),
		..Default::default()
	}
}

pub struct Ninja {}

impl Ninja {
	pub fn generate(
		project: Arc<Project>,
		build_dir: &Path,
		toolchain: Toolchain,
		profile: Profile,
		global_opts: GlobalOptions,
		target_platform: TargetPlatform,
	) -> Result<(), String> {
		let mut rules = NinjaRules::default();
		let mut build_lines = Vec::new();
		Ninja::generate_inner(
			&project,
			build_dir,
			&toolchain,
			&profile,
			&global_opts,
			&target_platform,
			&mut rules,
			&mut build_lines,
		)?;
		let mut rules_str = String::new();
		if let Some(c) = rules.compile_c_object {
			rules_str += &c.as_string();
		}
		if let Some(c) = rules.compile_cpp_object {
			rules_str += &c.as_string();
		}
		if let Some(c) = rules.link_static_lib {
			rules_str += &c.as_string();
		}
		if let Some(c) = rules.link_exe {
			rules_str += &c.as_string();
		}
		let build_ninja_path = build_dir.join("build.ninja");
		let mut f = match std::fs::File::create(build_ninja_path) {
			Ok(x) => x,
			Err(e) => return Err(format!("Error creating build.ninja: {}", e)),
		};
		if let Err(e) = f.write_all(rules_str.as_bytes()) {
			return Err(format!("Error writing to build.ninja: {}", e));
		}
		for line in build_lines {
			if let Err(e) = f.write_all(line.as_string().as_bytes()) {
				return Err(format!("Error writing to build.ninja: {}", e));
			}
		}
		Ok(())
	}

	fn generate_inner(
		project: &Arc<Project>,
		build_dir: &Path,
		toolchain: &Toolchain,
		profile: &Profile,
		global_opts: &GlobalOptions,
		target_platform: &TargetPlatform,
		rules: &mut NinjaRules,
		build_lines: &mut Vec<NinjaBuild>,
	) -> Result<(), String> {
		for subproject in &project.dependencies {
			Ninja::generate_inner(
				subproject,
				build_dir,
				toolchain,
				profile,
				global_opts,
				target_platform,
				rules,
				build_lines,
			)?;
		}

		let project_name = &project.info.name;
		let static_lib_ext = &target_platform.static_lib_ext;
		let obj_ext = &target_platform.obj_ext;
		if rules.link_static_lib.is_none() && !project.static_libraries.is_empty() {
			let static_linker = match &toolchain.static_linker {
				Some(x) => x,
				None => {
					return Err(format!(
						"No static linker specified in toolchain. A static linker is required to build \"{}\".",
						project.static_libraries.first().unwrap().name()
					))
				}
			};
			rules.link_static_lib = Some(link_static_lib(static_linker));
		}

		fn add_lib_source(
			src: &Path,
			lib: &StaticLibrary,
			out_tgt: String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) -> NinjaBuild {
			inputs.push(out_tgt.clone());
			let input = input_path(src, &lib.project().info.path);
			let mut includes = lib.public_includes_recursive();
			includes.extend_from_slice(&lib.private_includes());
			let mut defines = lib.public_defines_recursive();
			defines.extend_from_slice(&lib.private_defines());
			NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), transform_defines(&defines)),
					("FLAGS".to_string(), compile_options),
					(
						"INCLUDES".to_owned(),
						includes
							.iter()
							.map(|x| "-I".to_owned() + x.to_string_lossy().trim_start_matches(r"\\?\"))
							.collect(),
					),
				]),
			}
		}
		for lib in &project.static_libraries {
			let mut inputs = Vec::<String>::new();
			if !lib.sources.c.is_empty() {
				let c_compiler = get_c_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_c_object {
					rule
				} else {
					rules.compile_c_object = Some(compile_c_object(c_compiler));
					rules.compile_c_object.as_ref().unwrap()
				};
				let mut c_compile_opts = profile.c_compile_flags.clone();
				if let Some(c_std) = &global_opts.c_standard {
					c_compile_opts.push(c_compiler.c_std_flag(c_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = c_compiler.position_independent_code_flag() {
						c_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.sources.c {
					build_lines.push(add_lib_source(
						&src.full,
						lib,
						output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
						rule.clone(),
						c_compile_opts.clone(),
						&mut inputs,
					));
				}
			}

			if !lib.sources.cpp.is_empty() {
				let cpp_compiler = get_cpp_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_cpp_object {
					rule
				} else {
					rules.compile_cpp_object = Some(compile_cpp_object(cpp_compiler));
					rules.compile_cpp_object.as_ref().unwrap()
				};
				let mut cpp_compile_opts = profile.cpp_compile_flags.clone();
				if let Some(cpp_std) = &global_opts.cpp_standard {
					cpp_compile_opts.push(cpp_compiler.cpp_std_flag(cpp_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = cpp_compiler.position_independent_code_flag() {
						cpp_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.sources.cpp {
					build_lines.push(add_lib_source(
						&src.full,
						lib,
						output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
						rule.clone(),
						cpp_compile_opts.clone(),
						&mut inputs,
					));
				}
			}
			for link in &lib.public_links_recursive() {
				let mut add_path = |src: &str, ext: &str| {
					let link_path = output_path(build_dir, &link.project().info.name, src, ext);
					if !inputs.contains(&link_path) {
						inputs.push(link_path);
					}
				};
				match link {
					LinkPtr::Static(static_lib) => add_path(&static_lib.output_name(), static_lib_ext),
					LinkPtr::Object(obj_lib) => {
						for src in obj_lib.sources.iter() {
							add_path(&src.name, obj_ext);
						}
					}
					LinkPtr::Interface(_) => {}
				}
			}
			let out_name = output_path(build_dir, project_name, lib.output_name().as_ref(), static_lib_ext);
			let link_flags = Vec::new(); // TODO(Travers): Only for shared libs
							 // let mut link_flags = lib.public_link_flags_recursive();
							 // link_flags.extend_from_slice(&lib.private_link_flags());
			build_lines.push(NinjaBuild {
				inputs,
				output_targets: vec![out_name.clone()],
				rule: rules.link_static_lib.as_ref().unwrap().clone(),
				keyval_set: HashMap::from([
					("TARGET_FILE".to_string(), vec![out_name.clone()]),
					("LINK_FLAGS".to_string(), link_flags),
				]),
			});
			build_lines.push(NinjaBuild {
				inputs: vec![out_name],
				output_targets: vec![lib.name.clone()],
				rule: NinjaRule { name: "phony".to_owned(), ..Default::default() },
				keyval_set: HashMap::new(),
			});
		}

		fn add_obj_source(
			src: &Path,
			lib: &ObjectLibrary,
			out_tgt: String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) -> NinjaBuild {
			inputs.push(out_tgt.clone());
			let input = input_path(src, &lib.project().info.path);
			let mut includes = lib.public_includes_recursive();
			includes.extend_from_slice(&lib.private_includes());
			let mut defines = lib.public_defines_recursive();
			defines.extend_from_slice(&lib.private_defines());
			NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), transform_defines(&defines)),
					("FLAGS".to_string(), compile_options),
					(
						"INCLUDES".to_owned(),
						includes
							.iter()
							.map(|x| "-I".to_owned() + x.to_string_lossy().trim_start_matches(r"\\?\"))
							.collect(),
					),
				]),
			}
		}
		for lib in &project.object_libraries {
			let mut inputs = Vec::<String>::new();
			if !lib.sources.c.is_empty() {
				let c_compiler = get_c_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_c_object {
					rule
				} else {
					rules.compile_c_object = Some(compile_c_object(c_compiler));
					rules.compile_c_object.as_ref().unwrap()
				};
				let mut c_compile_opts = profile.c_compile_flags.clone();
				if let Some(c_std) = &global_opts.c_standard {
					c_compile_opts.push(c_compiler.c_std_flag(c_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = c_compiler.position_independent_code_flag() {
						c_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.sources.c {
					build_lines.push(add_obj_source(
						&src.full,
						lib,
						output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
						rule.clone(),
						c_compile_opts.clone(),
						&mut inputs,
					));
				}
			}

			if !lib.sources.cpp.is_empty() {
				let cpp_compiler = get_cpp_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_cpp_object {
					rule
				} else {
					rules.compile_cpp_object = Some(compile_cpp_object(cpp_compiler));
					rules.compile_cpp_object.as_ref().unwrap()
				};
				let mut cpp_compile_opts = profile.cpp_compile_flags.clone();
				if let Some(cpp_std) = &global_opts.cpp_standard {
					cpp_compile_opts.push(cpp_compiler.cpp_std_flag(cpp_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = cpp_compiler.position_independent_code_flag() {
						cpp_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.sources.cpp {
					build_lines.push(add_obj_source(
						&src.full,
						lib,
						output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
						rule.clone(),
						cpp_compile_opts.clone(),
						&mut inputs,
					));
				}
			}
			for link in &lib.public_links_recursive() {
				let mut add_path = || {
					let link_path =
						output_path(build_dir, &link.project().info.name, link.output_name().as_ref(), static_lib_ext);
					if !inputs.contains(&link_path) {
						inputs.push(link_path);
					}
				};
				match link {
					LinkPtr::Static(_) | LinkPtr::Object(_) => add_path(),
					LinkPtr::Interface(_) => {}
				}
			}
			// Omit phony rules for object libraries
		}
		fn add_exe_source(
			src: &Path,
			exe: &Executable,
			out_tgt: String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) -> NinjaBuild {
			inputs.push(out_tgt.clone());
			let input = input_path(src, &exe.project().info.path);
			let includes = exe.public_includes_recursive();
			let defines = exe.public_defines_recursive();
			NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), transform_defines(&defines)),
					("FLAGS".to_string(), compile_options),
					(
						"INCLUDES".to_owned(),
						includes
							.iter()
							.map(|x| "-I".to_owned() + x.to_string_lossy().trim_start_matches(r"\\?\"))
							.collect(),
					),
				]),
			}
		}
		if !project.executables.is_empty() {
			let mut link_exe_flags = Vec::new();
			let exe_linker = match &toolchain.exe_linker {
				Some(x) => x,
				None => {
					return Err(format!(
					"No executable linker specified in toolchain. An executable linker is required to build \"{}\".",
					project.executables.first().unwrap().name()
				))
				}
			};
			let rule_link_exe = if let Some(rule) = &rules.link_exe {
				rule
			} else {
				rules.link_exe = Some(link_exe(exe_linker.as_ref()));
				rules.link_exe.as_ref().unwrap()
			};
			if let Some(true) = global_opts.position_independent_code {
				if let Some(pie_flag) = exe_linker.position_independent_executable_flag() {
					link_exe_flags.push(pie_flag);
				}
			}
			for exe in &project.executables {
				log::debug!("   exe target: {}", exe.name);
				let mut inputs = Vec::<String>::new();
				if !exe.sources.c.is_empty() {
					let c_compiler = get_c_compiler(toolchain, &exe.name())?;
					let rule_compile_c = if let Some(rule) = &rules.compile_c_object {
						rule
					} else {
						rules.compile_c_object = Some(compile_c_object(c_compiler));
						rules.compile_c_object.as_ref().unwrap()
					};
					let mut c_compile_opts = profile.c_compile_flags.clone();
					if let Some(c_std) = &global_opts.c_standard {
						c_compile_opts.push(c_compiler.c_std_flag(c_std)?);
					}
					if let Some(true) = global_opts.position_independent_code {
						if let Some(fpic_flag) = c_compiler.position_independent_executable_flag() {
							c_compile_opts.push(fpic_flag);
						}
					}
					for src in &exe.sources.c {
						build_lines.push(add_exe_source(
							&src.full,
							exe,
							output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
							rule_compile_c.clone(),
							c_compile_opts.clone(),
							&mut inputs,
						));
					}
				}
				if !exe.sources.cpp.is_empty() {
					let cpp_compiler = get_cpp_compiler(toolchain, &exe.name())?;
					let rule_compile_cpp = if let Some(rule) = &rules.compile_cpp_object {
						rule
					} else {
						rules.compile_cpp_object = Some(compile_cpp_object(cpp_compiler));
						rules.compile_cpp_object.as_ref().unwrap()
					};
					let mut cpp_compile_opts = profile.cpp_compile_flags.clone();
					if let Some(cpp_std) = &global_opts.cpp_standard {
						cpp_compile_opts.push(cpp_compiler.cpp_std_flag(cpp_std)?);
					}
					if let Some(true) = global_opts.position_independent_code {
						if let Some(fpic_flag) = cpp_compiler.position_independent_executable_flag() {
							cpp_compile_opts.push(fpic_flag);
						}
					}
					for src in &exe.sources.cpp {
						build_lines.push(add_exe_source(
							&src.full,
							exe,
							output_path(build_dir, project_name, &src.name, &target_platform.obj_ext),
							rule_compile_cpp.clone(),
							cpp_compile_opts.clone(),
							&mut inputs,
						));
					}
				}
				for link in &exe.links {
					let mut add_path = |lnk: &LinkPtr, src: &str, ext: &str| {
						if src.contains("mylib") {
							println!("add_path {} {}", src, lnk.project().info.name);
						}
						let link_path = output_path(build_dir, &lnk.project().info.name, src, ext);
						if !inputs.contains(&link_path) {
							inputs.push(link_path);
						}
					};
					match link {
						LinkPtr::Static(_) => add_path(link, &link.output_name(), static_lib_ext),
						LinkPtr::Object(obj_lib) => {
							for src in obj_lib.sources.iter() {
								add_path(link, &src.name, obj_ext);
							}
						}
						LinkPtr::Interface(_) => {}
					}
					for translink in &link.public_links_recursive() {
						match translink {
							LinkPtr::Static(_) => add_path(translink, &translink.output_name(), static_lib_ext),
							LinkPtr::Object(obj_lib) => {
								for src in obj_lib.sources.iter() {
									add_path(translink, &src.name, obj_ext);
								}
							}
							LinkPtr::Interface(_) => {}
						}
					}
				}
				let mut link_flags = link_exe_flags.clone();
				link_flags.extend(exe.link_flags_recursive());
				let out_name = output_path(build_dir, project_name, exe.name.as_ref(), &target_platform.exe_ext);
				build_lines.push(NinjaBuild {
					inputs,
					output_targets: vec![out_name.clone()],
					rule: rule_link_exe.clone(),
					keyval_set: HashMap::from([
						("TARGET_FILE".to_string(), vec![out_name.clone()]),
						("LINK_FLAGS".to_string(), link_flags),
					]),
				});
				build_lines.push(NinjaBuild {
					inputs: vec![out_name],
					output_targets: vec![exe.name.clone()],
					rule: NinjaRule { name: "phony".to_owned(), ..Default::default() },
					keyval_set: HashMap::new(),
				});
			}
		}
		Ok(())
	}
}

fn get_c_compiler<'a>(toolchain: &'a Toolchain, name: &str) -> Result<&'a dyn Compiler, String> {
	match toolchain.c_compiler {
		Some(ref x) => Ok(x.as_ref()),
		None => Err(format!(
			"No C compiler specified in toolchain. A C compiler is required to build C sources in \"{}\".",
			name
		)),
	}
}

fn get_cpp_compiler<'a>(toolchain: &'a Toolchain, name: &str) -> Result<&'a dyn Compiler, String> {
	match toolchain.cpp_compiler {
		Some(ref x) => Ok(x.as_ref()),
		None => Err(format!(
			"No C++ compiler specified in toolchain. A C++ compiler is required to build C++ sources in \"{}\".",
			name
		)),
	}
}

#[test]
fn test_position_independent_code() {
	use crate::misc::{SourcePath, Sources};
	use core::default::Default;
	use std::path::PathBuf;

	struct TestCompiler {}
	impl Compiler for TestCompiler {
		fn id(&self) -> String {
			"clang".to_owned()
		}
		fn version(&self) -> String {
			"17.0.0".to_owned()
		}
		fn cmd(&self) -> Vec<String> {
			vec!["clang".to_owned()]
		}
		fn out_flag(&self) -> String {
			"-o".to_owned()
		}
		fn c_std_flag(&self, std: &str) -> Result<String, String> {
			match std {
				"11" => Ok("-std=c11".to_owned()),
				"17" => Ok("-std=c17".to_owned()),
				_ => Err(format!("C standard not supported by compiler: {std}")),
			}
		}
		fn cpp_std_flag(&self, std: &str) -> Result<String, String> {
			match std {
				"11" => Ok("-std=c++11".to_owned()),
				"14" => Ok("-std=c++14".to_owned()),
				"17" => Ok("-std=c++17".to_owned()),
				"20" => Ok("-std=c++20".to_owned()),
				"23" => Ok("-std=c++23".to_owned()),
				_ => Err(format!("C++ standard not supported by compiler: {std}")),
			}
		}
		fn position_independent_code_flag(&self) -> Option<String> {
			Some("-fPIC".to_owned())
		}
		fn position_independent_executable_flag(&self) -> Option<String> {
			Some("-fPIE".to_owned())
		}
	}
	impl ExeLinker for TestCompiler {
		fn cmd(&self) -> Vec<String> {
			vec!["clang".to_owned()]
		}
		fn position_independent_executable_flag(&self) -> Option<String> {
			Some("-pie".to_owned())
		}
	}
	let mut add_lib: Option<Arc<StaticLibrary>> = None;
	let mut create_lib = |weak_parent: &std::sync::Weak<Project>| -> Arc<StaticLibrary> {
		match &add_lib {
			Some(x) => x.clone(),
			None => {
				add_lib = Some(Arc::new(StaticLibrary {
					parent_project: weak_parent.clone(),
					name: "add".to_owned(),
					sources: Sources {
						cpp: vec![SourcePath { full: PathBuf::from("add.cpp"), name: "add.cpp".to_owned() }],
						..Default::default()
					},
					link_public: Vec::new(),
					link_private: Vec::new(),
					include_dirs_public: Vec::new(),
					include_dirs_private: Vec::new(),
					defines_public: Vec::new(),
					link_flags_public: Vec::new(),
					output_name: None,
				}));
				add_lib.as_ref().unwrap().clone()
			}
		}
	};
	let project = Arc::new_cyclic(|weak_parent| Project {
		info: Arc::new(crate::project::ProjectInfo { name: "test_project".to_owned(), path: PathBuf::from(".") }),
		dependencies: Vec::new(),
		executables: vec![Arc::new(Executable {
			parent_project: weak_parent.clone(),
			name: "main".to_owned(),
			sources: Sources {
				cpp: vec![SourcePath { full: PathBuf::from("main.cpp"), name: "main.cpp".to_owned() }],
				..Default::default()
			},
			links: vec![LinkPtr::Static(create_lib(weak_parent))],
			include_dirs: Vec::new(),
			defines: Vec::new(),
			link_flags: Vec::new(),
			output_name: None,
		})],
		static_libraries: vec![create_lib(weak_parent)],
		object_libraries: Vec::new(),
		interface_libraries: Vec::new(),
	});
	let toolchain = Toolchain {
		c_compiler: Some(Box::new(TestCompiler {})),
		cpp_compiler: Some(Box::new(TestCompiler {})),
		static_linker: Some(vec!["llvm-ar".to_owned()]),
		exe_linker: Some(Box::new(TestCompiler {})),
		profile: Default::default(),
	};
	let profile = Default::default();
	let global_opts = GlobalOptions {
		c_standard: Some("17".to_owned()),
		cpp_standard: Some("17".to_owned()),
		position_independent_code: Some(true),
	};
	let target_platform = TargetPlatform {
		obj_ext: ".o".to_owned(),
		static_lib_ext: ".a".to_owned(),
		exe_ext: String::new(),
	};
	let mut rules = NinjaRules::default();
	let mut build_lines = Vec::new();
	let result = Ninja::generate_inner(
		&project,
		&PathBuf::from("build"),
		&toolchain,
		&profile,
		&global_opts,
		&target_platform,
		&mut rules,
		&mut build_lines,
	);

	assert!(result.is_ok(), "{}", result.unwrap_err());

	assert_eq!(build_lines.len(), 6);

	let add_cpp_path = PathBuf::from(".").join("add.cpp").to_string_lossy().to_string();
	let add_cpp_rules = build_lines
		.iter()
		.filter(|x| x.inputs.first().unwrap() == &add_cpp_path)
		.collect::<Vec<_>>();
	assert_eq!(add_cpp_rules.len(), 1);

	assert_eq!(
		add_cpp_rules
			.first()
			.unwrap()
			.keyval_set
			.get("FLAGS")
			.unwrap()
			.iter()
			.filter(|x| *x == "-fPIC")
			.count(),
		1
	);

	let main_cpp_path = PathBuf::from(".").join("main.cpp").to_string_lossy().to_string();
	let main_cpp_rules = build_lines
		.iter()
		.filter(|x| x.inputs.first().unwrap() == &main_cpp_path)
		.collect::<Vec<_>>();
	assert_eq!(add_cpp_rules.len(), 1);

	assert_eq!(
		main_cpp_rules
			.first()
			.unwrap()
			.keyval_set
			.get("FLAGS")
			.unwrap()
			.iter()
			.filter(|x| *x == "-fPIE")
			.count(),
		1
	);

	let main_out_path = PathBuf::from("build")
		.join("test_project")
		.join("main")
		.to_string_lossy()
		.to_string();
	let main_exe_rules = build_lines
		.iter()
		.filter(|x| {
			println!("out: {}", x.output_targets.first().unwrap());
			x.output_targets.first().unwrap() == &main_out_path
		})
		.collect::<Vec<_>>();
	assert_eq!(add_cpp_rules.len(), 1);

	assert_eq!(
		main_exe_rules
			.first()
			.unwrap()
			.keyval_set
			.get("LINK_FLAGS")
			.unwrap()
			.iter()
			.filter(|x| *x == "-pie")
			.count(),
		1
	);
}
