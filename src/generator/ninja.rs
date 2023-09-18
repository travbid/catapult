use core::default::Default;
use std::{
	collections::HashMap,
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use log;

use super::{TargetPlatform, Toolchain};
use crate::{
	executable::Executable,
	link_type::LinkPtr,
	project::Project,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
	toolchain::compiler::{Compiler, ExeLinker},
	GlobalOptions,
};

fn input_path(src: &str, project_path: &Path) -> String {
	let src_path = PathBuf::from(src);
	if src_path.is_relative() {
		project_path.join(src)
	} else {
		src_path
	}
	.to_str()
	.unwrap()
	.to_owned()
}

fn output_path(build_dir: &Path, project_name: &str, src: &str, ext: &str) -> String {
	build_dir
		.join(project_name)
		.join(src.to_owned() + ext)
		.to_str()
		.unwrap()
		.to_owned()
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
		global_opts: GlobalOptions,
		target_platform: TargetPlatform,
	) -> Result<(), String> {
		let mut rules = NinjaRules::default();
		let mut build_lines = Vec::new();
		Ninja::generate_inner(
			&project,
			build_dir,
			&toolchain,
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
		global_opts: &GlobalOptions,
		target_platform: &TargetPlatform,
		rules: &mut NinjaRules,
		build_lines: &mut Vec<NinjaBuild>,
	) -> Result<(), String> {
		for subproject in &project.dependencies {
			Ninja::generate_inner(subproject, build_dir, toolchain, global_opts, target_platform, rules, build_lines)?;
		}

		let project_name = &project.info.name;
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
			src: &str,
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
			let defines = defines.iter().map(|x| "-D".to_string() + x).collect();
			NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), defines),
					("FLAGS".to_string(), compile_options),
					("INCLUDES".to_owned(), includes.iter().map(|x| "-I".to_owned() + x).collect()),
				]),
			}
		}
		for lib in &project.static_libraries {
			let mut inputs = Vec::<String>::new();
			if !lib.c_sources.is_empty() {
				let c_compiler = get_c_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_c_object {
					rule
				} else {
					rules.compile_c_object = Some(compile_c_object(c_compiler));
					rules.compile_c_object.as_ref().unwrap()
				};
				let mut c_compile_opts = Vec::new();
				if let Some(c_std) = &global_opts.c_standard {
					c_compile_opts.push(c_compiler.c_std_flag(c_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = c_compiler.position_independent_code_flag() {
						c_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.c_sources {
					build_lines.push(add_lib_source(
						src,
						lib,
						output_path(build_dir, project_name, src, &target_platform.obj_ext),
						rule.clone(),
						c_compile_opts.clone(),
						&mut inputs,
					));
				}
			}

			if !lib.cpp_sources.is_empty() {
				let cpp_compiler = get_cpp_compiler(toolchain, &lib.name())?;
				let rule = if let Some(rule) = &rules.compile_cpp_object {
					rule
				} else {
					rules.compile_cpp_object = Some(compile_cpp_object(cpp_compiler));
					rules.compile_cpp_object.as_ref().unwrap()
				};
				let mut cpp_compile_opts = Vec::new();
				if let Some(cpp_std) = &global_opts.cpp_standard {
					cpp_compile_opts.push(cpp_compiler.cpp_std_flag(cpp_std)?);
				}
				if let Some(true) = global_opts.position_independent_code {
					if let Some(fpic_flag) = cpp_compiler.position_independent_code_flag() {
						cpp_compile_opts.push(fpic_flag);
					}
				}
				for src in &lib.cpp_sources {
					build_lines.push(add_lib_source(
						src,
						lib,
						output_path(build_dir, project_name, src, &target_platform.obj_ext),
						rule.clone(),
						cpp_compile_opts.clone(),
						&mut inputs,
					));
				}
			}
			for link in &lib.public_links_recursive() {
				match link {
					LinkPtr::Static(x) => {
						let link = output_path(
							build_dir,
							&x.project().info.name,
							&link.output_name(),
							&target_platform.static_lib_ext,
						);
						if !inputs.contains(&link) {
							inputs.push(link);
						}
					}
					LinkPtr::Interface(_) => {}
				};
			}
			let out_name = output_path(build_dir, project_name, &lib.output_name(), &target_platform.static_lib_ext);
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
		fn add_exe_source(
			src: &str,
			exe: &Executable,
			out_tgt: String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) -> NinjaBuild {
			inputs.push(out_tgt.clone());
			let input = input_path(src, &exe.project().info.path);
			let includes = exe.public_includes_recursive();
			let defines = exe
				.public_defines_recursive()
				.iter()
				.map(|x| "-D".to_string() + x)
				.collect();
			NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), defines),
					("FLAGS".to_string(), compile_options),
					("INCLUDES".to_owned(), includes.iter().map(|x| "-I".to_owned() + x).collect()),
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
				if !exe.c_sources.is_empty() {
					let c_compiler = get_c_compiler(toolchain, &exe.name())?;
					let rule_compile_c = if let Some(rule) = &rules.compile_c_object {
						rule
					} else {
						rules.compile_c_object = Some(compile_c_object(c_compiler));
						rules.compile_c_object.as_ref().unwrap()
					};
					let mut c_compile_opts = Vec::new();
					if let Some(c_std) = &global_opts.c_standard {
						c_compile_opts.push(c_compiler.c_std_flag(c_std)?);
					}
					if let Some(true) = global_opts.position_independent_code {
						if let Some(fpic_flag) = c_compiler.position_independent_executable_flag() {
							c_compile_opts.push(fpic_flag);
						}
					}
					for src in &exe.c_sources {
						build_lines.push(add_exe_source(
							src,
							exe,
							output_path(build_dir, project_name, src, &target_platform.obj_ext),
							rule_compile_c.clone(),
							c_compile_opts.clone(),
							&mut inputs,
						));
					}
				}
				if !exe.cpp_sources.is_empty() {
					let cpp_compiler = get_cpp_compiler(toolchain, &exe.name())?;
					let rule_compile_cpp = if let Some(rule) = &rules.compile_cpp_object {
						rule
					} else {
						rules.compile_cpp_object = Some(compile_cpp_object(cpp_compiler));
						rules.compile_cpp_object.as_ref().unwrap()
					};
					let mut cpp_compile_opts = Vec::new();
					if let Some(cpp_std) = &global_opts.cpp_standard {
						cpp_compile_opts.push(cpp_compiler.cpp_std_flag(cpp_std)?);
					}
					if let Some(true) = global_opts.position_independent_code {
						if let Some(fpic_flag) = cpp_compiler.position_independent_executable_flag() {
							cpp_compile_opts.push(fpic_flag);
						}
					}
					for src in &exe.cpp_sources {
						build_lines.push(add_exe_source(
							src,
							exe,
							output_path(build_dir, project_name, src, &target_platform.obj_ext),
							rule_compile_cpp.clone(),
							cpp_compile_opts.clone(),
							&mut inputs,
						));
					}
				}
				for link in &exe.links {
					match link {
						LinkPtr::Static(x) => {
							let lib_path = output_path(
								build_dir,
								&x.project().info.name,
								&x.output_name(),
								&target_platform.static_lib_ext,
							);
							if !inputs.contains(&lib_path) {
								inputs.push(lib_path);
							}
						}
						LinkPtr::Interface(_) => {}
					}
					for translink in link.public_links_recursive() {
						match translink {
							LinkPtr::Static(x) => {
								let lib_path = output_path(
									build_dir,
									&x.project().info.name,
									&x.output_name(),
									&target_platform.static_lib_ext,
								);
								if !inputs.contains(&lib_path) {
									inputs.push(lib_path);
								}
							}
							LinkPtr::Interface(_) => {}
						}
					}
				}
				let mut link_flags = link_exe_flags.clone();
				link_flags.extend(exe.link_flags_recursive());
				let out_name = output_path(build_dir, project_name, &exe.name, &target_platform.exe_ext);
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
