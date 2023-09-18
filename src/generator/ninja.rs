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

#[derive(Clone)]
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
  command = {}
"#,
			self.name,
			self.command.join(" ")
		);
		if !self.depfile.is_empty() {
			ret += "  depfile = ";
			ret += &self.depfile.join(" ");
		}
		if !self.deps.is_empty() {
			ret += "  deps = ";
			ret += &self.deps.join(" ");
		}
		if let Some(desc) = &self.description {
			ret += "  description = ";
			ret += desc;
		}
		if let Some(dyndep) = &self.dyndep {
			ret += "  dyndep = ";
			ret += dyndep;
		}
		if self.generator {
			ret += "  generator = 1";
		}
		if let Some(restat) = &self.restat {
			ret += "  restat = ";
			ret += restat;
		}
		if let Some(rspfile) = &self.rspfile {
			ret += "  rspfile = ";
			ret += &rspfile.rspfile;
			ret += "  rspfilecontent = ";
			ret += &rspfile.rspfilecontent;
		}
		ret += "\n";
		ret
	}
}

impl Default for NinjaRule {
	fn default() -> Self {
		NinjaRule {
			name: String::new(),
			command: Vec::new(),
			depfile: Vec::new(),
			deps: Vec::new(),
			description: None,
			dyndep: None,
			generator: false,
			restat: None,
			rspfile: None,
		}
	}
}

struct NinjaRules {
	compile_c_object: Option<NinjaRule>,
	compile_cpp_object: Option<NinjaRule>,
	link_static_lib: Option<NinjaRule>,
	link_exe: Option<NinjaRule>,
}

impl Default for NinjaRules {
	fn default() -> Self {
		NinjaRules {
			compile_c_object: None,
			compile_cpp_object: None,
			link_static_lib: None,
			link_exe: None,
		}
	}
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

fn concat_vecs(vec_a: &[String], vec_b: &[String]) -> Vec<String> {
	let mut ret = vec_a.to_owned();
	ret.extend(vec_b.iter().cloned());
	ret
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
		..Default::default()
	}
}
fn link_static_lib(static_linker: &[String]) -> NinjaRule {
	let mut command = static_linker.to_owned();
	command.extend(vec!["$TARGET_FILE".to_string(), "$LINK_FLAGS".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("link_static_lib"),
		command,
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
		name: String::from("compile_exe"),
		command,
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
		let mut out_str = String::new();
		Ninja::generate_inner(
			&project,
			build_dir,
			&toolchain,
			&global_opts,
			&target_platform,
			&mut rules,
			&mut out_str,
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
		if let Err(e) = f.write_all(out_str.as_bytes()) {
			return Err(format!("Error writing to build.ninja: {}", e));
		}
		Ok(())
	}
	fn generate_inner(
		// projects: BTreeMap<String, Arc<Project>>,
		project: &Arc<Project>,
		build_dir: &Path,
		toolchain: &Toolchain,
		global_opts: &GlobalOptions,
		target_platform: &TargetPlatform,
		rules: &mut NinjaRules,
		out_str: &mut String,
	) -> Result<(), String> {
		for subproject in &project.dependencies {
			Ninja::generate_inner(subproject, build_dir, toolchain, global_opts, target_platform, rules, out_str)?;
		}

		let project_name = &project.info.name;
		if rules.link_static_lib.is_none() && !project.static_libraries.is_empty() {
			rules.link_static_lib = Some(link_static_lib(&toolchain.static_linker));
		}
		let mut c_compile_opts = Vec::new();
		if let Some(c_std) = &global_opts.c_standard {
			c_compile_opts.push(toolchain.c_compiler.c_std_flag(c_std)?);
		}
		let mut cpp_compile_opts = Vec::new();
		if let Some(cpp_std) = &global_opts.c_standard {
			cpp_compile_opts.push(toolchain.cpp_compiler.cpp_std_flag(cpp_std)?);
		}
		let mut c_compile_lib_opts = Vec::new();
		let mut cpp_compile_lib_opts = Vec::new();
		let mut c_compile_exe_opts = Vec::new();
		let mut cpp_compile_exe_opts = Vec::new();
		if let Some(true) = global_opts.position_independent_code {
			if let Some(fpic_flag) = toolchain.c_compiler.position_independent_code_flag() {
				c_compile_lib_opts.push(fpic_flag);
			}
			if let Some(fpic_flag) = toolchain.cpp_compiler.position_independent_code_flag() {
				cpp_compile_lib_opts.push(fpic_flag);
			}
			if let Some(fpic_flag) = toolchain.c_compiler.position_independent_executable_flag() {
				c_compile_exe_opts.push(fpic_flag);
			}
			if let Some(fpic_flag) = toolchain.cpp_compiler.position_independent_executable_flag() {
				cpp_compile_exe_opts.push(fpic_flag);
			}
		}
		fn add_lib_source(
			src: &str,
			lib: &StaticLibrary,
			build_dir: &Path,
			project_name: &str,
			target_platform: &TargetPlatform,
			out_str: &mut String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) {
			let input = input_path(src, &lib.project().info.path);
			let out_tgt = output_path(build_dir, project_name, src, &target_platform.obj_ext);
			let mut includes = lib.public_includes_recursive();
			includes.extend_from_slice(&lib.private_includes());
			let mut defines = lib.public_defines_recursive();
			defines.extend_from_slice(&lib.private_defines());
			let defines = defines.iter().map(|x| "-D".to_string() + x).collect();
			*out_str += &NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt.clone()],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), defines),
					("FLAGS".to_string(), compile_options),
					("INCLUDES".to_owned(), includes.iter().map(|x| "-I".to_owned() + x).collect()),
				]),
			}
			.as_string();
			inputs.push(out_tgt);
		}
		for lib in &project.static_libraries {
			let mut inputs = Vec::<String>::new();
			if rules.compile_c_object.is_none() && !lib.c_sources.is_empty() {
				rules.compile_c_object = Some(compile_c_object(toolchain.c_compiler.as_ref()));
			}
			if rules.compile_cpp_object.is_none() && !lib.cpp_sources.is_empty() {
				rules.compile_cpp_object = Some(compile_cpp_object(toolchain.cpp_compiler.as_ref()));
			}
			for src in &lib.c_sources {
				add_lib_source(
					src,
					lib,
					build_dir,
					project_name,
					target_platform,
					out_str,
					rules.compile_c_object.as_ref().unwrap().clone(),
					concat_vecs(&c_compile_opts, &c_compile_lib_opts),
					&mut inputs,
				);
			}
			for src in &lib.cpp_sources {
				add_lib_source(
					src,
					lib,
					build_dir,
					project_name,
					target_platform,
					out_str,
					rules.compile_cpp_object.as_ref().unwrap().clone(),
					concat_vecs(&cpp_compile_opts, &cpp_compile_lib_opts),
					&mut inputs,
				);
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
			*out_str += &NinjaBuild {
				inputs,
				output_targets: vec![out_name.clone()],
				rule: rules.link_static_lib.as_ref().unwrap().clone(),
				keyval_set: HashMap::from([
					("TARGET_FILE".to_string(), vec![out_name.clone()]),
					("LINK_FLAGS".to_string(), link_flags),
				]),
			}
			.as_string();
			*out_str += &format!("build {}: phony {}\n\n", lib.name, out_name.replace(':', "$:"));
		}
		fn add_exe_source(
			src: &str,
			exe: &Executable,
			build_dir: &Path,
			project_name: &str,
			target_platform: &TargetPlatform,
			out_str: &mut String,
			rule: NinjaRule,
			compile_options: Vec<String>,
			inputs: &mut Vec<String>,
		) {
			let input = input_path(src, &exe.project().info.path);
			let out_tgt = output_path(build_dir, project_name, src, &target_platform.obj_ext);
			let includes = exe.public_includes_recursive();
			let defines = exe
				.public_defines_recursive()
				.iter()
				.map(|x| "-D".to_string() + x)
				.collect();
			*out_str += &NinjaBuild {
				inputs: vec![input],
				output_targets: vec![out_tgt.clone()],
				rule,
				keyval_set: HashMap::from([
					("DEFINES".to_string(), defines),
					("FLAGS".to_string(), compile_options),
					("INCLUDES".to_owned(), includes.iter().map(|x| "-I".to_owned() + x).collect()),
				]),
			}
			.as_string();
			inputs.push(out_tgt);
		}
		if rules.link_exe.is_none() && !project.executables.is_empty() {
			rules.link_exe = Some(link_exe(toolchain.exe_linker.as_ref()));
		}
		for exe in &project.executables {
			log::debug!("   exe target: {}", exe.name);
			let mut inputs = Vec::<String>::new();
			for src in &exe.c_sources {
				add_exe_source(
					src,
					exe,
					build_dir,
					project_name,
					target_platform,
					out_str,
					rules.compile_c_object.as_ref().unwrap().clone(),
					concat_vecs(&c_compile_opts, &c_compile_exe_opts),
					&mut inputs,
				);
			}
			for src in &exe.cpp_sources {
				add_exe_source(
					src,
					exe,
					build_dir,
					project_name,
					target_platform,
					out_str,
					rules.compile_cpp_object.as_ref().unwrap().clone(),
					concat_vecs(&cpp_compile_opts, &cpp_compile_exe_opts),
					&mut inputs,
				);
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
			let mut link_flags = exe.link_flags_recursive();
			if let Some(pie_flag) = toolchain.exe_linker.position_independent_executable_flag() {
				link_flags.push(pie_flag);
			}
			let out_name = output_path(build_dir, project_name, &exe.name, &target_platform.exe_ext);
			*out_str += &NinjaBuild {
				inputs,
				output_targets: vec![out_name.clone()],
				rule: rules.link_exe.as_ref().unwrap().clone(),
				keyval_set: HashMap::from([
					("TARGET_FILE".to_string(), vec![out_name.clone()]),
					("LINK_FLAGS".to_string(), link_flags),
				]),
			}
			.as_string();
			*out_str += &format!("build {}: phony {}\n\n", exe.name, out_name.replace(':', "$:"));
		}
		Ok(())
	}
}
