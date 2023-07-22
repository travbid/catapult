use core::default::Default;
use std::{
	collections::HashMap,
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use super::{BuildTools, TargetPlatform};
use crate::{
	executable::Executable,
	library::Library,
	project::Project,
	target::{LinkTarget, Target},
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
		ret += "\n\n";
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
		let inputs = self
			.inputs
			.iter()
			.map(|i| i.replace(':', "&:"))
			.collect::<Vec<String>>();
		let mut ret = String::new();
		ret += &format!("build {}: {} {}\n", self.output_targets.join(" "), self.rule.name, inputs.join(" "));
		for (key, values) in &self.keyval_set {
			if !values.is_empty() {
				ret += &format!("  {key} = {}\n", values.join(" "));
			}
		}
		ret += "\n\n";
		ret
	}
}

fn compile_c_object(compiler: &[String], out_flag: &str) -> NinjaRule {
	let mut command = compiler.to_owned();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	// command.extend(compiler.compiler_flags(msvc_runtime));
	command.extend(vec![out_flag.to_owned(), "$out".to_owned()]);
	command.extend(vec!["-c".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("compile_c_object"),
		command,
		..Default::default()
	}
}
fn compile_cpp_object(compiler: &[String], out_flag: &str) -> NinjaRule {
	let mut command = compiler.to_owned();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	command.extend(vec![out_flag.to_owned(), "$out".to_owned()]);
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
fn link_exe(exe_linker: &[String]) -> NinjaRule {
	let mut command = exe_linker.to_owned();
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
		build_dir: PathBuf,
		build_tools: BuildTools,
		compile_options: Vec<String>,
		target_platform: TargetPlatform,
	) -> Result<(), String> {
		let mut rules = NinjaRules::default();
		let mut out_str = String::new();
		Ninja::generate_inner(
			&project,
			&build_dir,
			&build_tools,
			&compile_options,
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
		build_tools: &BuildTools,
		compile_options: &Vec<String>,
		target_platform: &TargetPlatform,
		rules: &mut NinjaRules,
		out_str: &mut String,
	) -> Result<(), String> {
		for subproject in &project.dependencies {
			Ninja::generate_inner(
				subproject,
				build_dir,
				build_tools,
				compile_options,
				target_platform,
				rules,
				out_str,
			)?;
		}

		let project_name = &project.info.name;
		if rules.link_static_lib.is_none() && !project.libraries.is_empty() {
			rules.link_static_lib = Some(link_static_lib(&build_tools.static_linker));
		}
		fn add_lib_source(
			src: &str,
			lib: &Library,
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
		for lib in &project.libraries {
			let mut inputs = Vec::<String>::new();
			if rules.compile_c_object.is_none() && !lib.c_sources.is_empty() {
				rules.compile_c_object = Some(compile_c_object(&build_tools.c_compiler, &build_tools.out_flag));
			}
			if rules.compile_cpp_object.is_none() && !lib.cpp_sources.is_empty() {
				rules.compile_cpp_object = Some(compile_cpp_object(&build_tools.cpp_compiler, &build_tools.out_flag));
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
					compile_options.clone(),
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
					compile_options.clone(),
					&mut inputs,
				);
			}
			for link in lib.public_links_recursive() {
				let link =
					output_path(build_dir, &link.project().info.name, &link.output_name(), &target_platform.obj_ext);
				inputs.push(link);
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
			*out_str += &format!("build {}: phony {}\n\n", lib.name, out_name);
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
			rules.link_exe = Some(link_exe(&build_tools.exe_linker));
		}
		for exe in &project.executables {
			println!("   target: {}", exe.name);
			let mut object_names = Vec::<String>::new();
			for src in &exe.c_sources {
				add_exe_source(
					src,
					exe,
					build_dir,
					project_name,
					target_platform,
					out_str,
					rules.compile_c_object.as_ref().unwrap().clone(),
					compile_options.clone(),
					&mut object_names,
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
					compile_options.clone(),
					&mut object_names,
				);
			}
			for link in &exe.links {
				object_names.push(output_path(
					build_dir,
					&link.project().info.name,
					&link.output_name(),
					&target_platform.static_lib_ext,
				));
			}
			let link_flags = exe.link_flags_recursive();
			let out_name = output_path(build_dir, project_name, &exe.name, &target_platform.exe_ext);
			*out_str += &NinjaBuild {
				inputs: object_names,
				output_targets: vec![out_name.clone()],
				rule: rules.link_exe.as_ref().unwrap().clone(),
				keyval_set: HashMap::from([
					("TARGET_FILE".to_string(), vec![out_name.clone()]),
					("LINK_FLAGS".to_string(), link_flags),
				]),
			}
			.as_string();
			*out_str += &format!("build {}: phony {}\n\n", exe.name, out_name);
		}
		Ok(())
	}
}
