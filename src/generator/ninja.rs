use core::default::Default;
use std::{
	collections::{HashMap, HashSet},
	hash::Hash,
	io::Write,
	path::{Path, PathBuf}, //
	sync::Arc,
};

use log;

use super::{TargetPlatform, Toolchain};
use crate::{
	executable::Executable,
	link_type::LinkPtr,
	misc::{join_parent, Sources},
	object_library::ObjectLibrary,
	project::Project,
	starlark_context::{StarContext, StarContextCompiler},
	starlark_generator::eval_vars,
	starlark_object_library::StarGeneratorVars,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
	toolchain::{
		compiler::{Assembler, Compiler, ExeLinker},
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

fn output_subfolder_path(build_dir: &Path, project_name: &str, subfolder: &str, src: &str, ext: &str) -> String {
	build_dir
		.join(project_name)
		.join(subfolder.to_owned() + ".dir")
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
#[allow(dead_code)]
enum NinjaDeps {
	Gcc,
	Msvc, // `deps = msvc` is unused until catapult supports using cl.exe with Ninja
}

impl NinjaDeps {
	fn as_str(&self) -> &'static str {
		match self {
			Self::Gcc => "gcc",
			Self::Msvc => "msvc",
		}
	}
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
	depfile: Option<String>,
	deps: Option<NinjaDeps>,
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
		if let Some(depfile) = &self.depfile {
			ret += "\n  depfile = ";
			ret += depfile;
		}
		if let Some(dep) = &self.deps {
			ret += "\n  deps = ";
			ret += dep.as_str();
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
	assemble_nasm_object: Option<NinjaRule>,
	link_static_lib: Option<NinjaRule>,
	link_exe: Option<NinjaRule>,
}

struct NinjaBuild {
	inputs: Vec<String>,
	output_targets: Vec<String>,
	rule_name: String,
	keyval_set: HashMap<String, Vec<String>>,
}

impl NinjaBuild {
	fn as_string(&self) -> String {
		let mut ret = String::new();
		ret += &format!(
			"build {}: {} {}\n",
			self.output_targets.join(" ").replace(':', "$:"),
			self.rule_name,
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
	command.extend(compiler.depfile_flags("$out", "$DEP_FILE"));
	command.extend(vec![compiler.out_flag(), "$out".to_owned()]);
	command.extend(vec!["-c".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("compile_c_object"),
		command,
		depfile: Some("$DEP_FILE".to_owned()),
		deps: Some(NinjaDeps::Gcc),
		description: Some("Compiling C object $out".to_owned()),
		..Default::default()
	}
}
fn compile_cpp_object(compiler: &dyn Compiler) -> NinjaRule {
	let mut command = compiler.cmd();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	command.extend(compiler.depfile_flags("$out", "$DEP_FILE"));
	command.extend(vec![compiler.out_flag(), "$out".to_owned()]);
	command.extend(vec!["-c".to_string(), "$in".to_string()]);
	NinjaRule {
		name: String::from("compile_cpp_object"),
		command,
		depfile: Some("$DEP_FILE".to_owned()),
		deps: Some(NinjaDeps::Gcc),
		description: Some("Compiling C++ object $out".to_owned()),
		..Default::default()
	}
}
fn assemble_nasm_object(assembler: &dyn Assembler) -> NinjaRule {
	let mut command = assembler.cmd();
	command.extend(vec!["$DEFINES".to_string(), "$INCLUDES".to_string(), "$FLAGS".to_string()]);
	command.extend(assembler.depfile_flags("$out", "$DEP_FILE"));
	command.extend(vec![assembler.out_flag(), "$out".to_owned()]);
	command.extend(vec!["$in".to_string()]);
	NinjaRule {
		name: String::from("assemble_nasm_object"),
		command,
		depfile: Some("$DEP_FILE".to_owned()),
		deps: Some(NinjaDeps::Gcc),
		description: Some("Assembling NASM object $out".to_owned()),
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

struct GeneratorOpts {
	build_dir: PathBuf,
	toolchain: Toolchain,
	profile: Profile,
	global_opts: GlobalOptions,
	target_platform: TargetPlatform,
	star_context: StarContext,
}

struct SourceData {
	includes: Vec<PathBuf>,
	defines: Vec<String>,
}

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
		let star_context = StarContext {
			c_compiler: toolchain
				.c_compiler
				.as_ref()
				.map(|compiler| StarContextCompiler { target_triple: compiler.target() }),
			cpp_compiler: toolchain
				.cpp_compiler
				.as_ref()
				.map(|compiler| StarContextCompiler { target_triple: compiler.target() }),
		};
		let generator_opts = GeneratorOpts {
			build_dir: build_dir.to_owned(),
			toolchain,
			profile,
			global_opts,
			target_platform,
			star_context,
		};
		let mut link_targets = HashMap::new();
		Ninja::generate_inner(&project, &generator_opts, &mut rules, &mut build_lines, &mut link_targets)?;
		let mut rules_str = String::new();
		if let Some(c) = rules.compile_c_object {
			rules_str += &c.as_string();
		}
		if let Some(c) = rules.compile_cpp_object {
			rules_str += &c.as_string();
		}
		if let Some(c) = rules.assemble_nasm_object {
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
		generator_opts: &GeneratorOpts,
		rules: &mut NinjaRules,
		build_lines: &mut Vec<NinjaBuild>,
		link_targets: &mut HashMap<LinkPtr, Vec<String>>,
	) -> Result<(), String> {
		log::debug!("Ninja::generate_inner() build_dir: {}", generator_opts.build_dir.display());

		for subproject in &project.dependencies {
			Ninja::generate_inner(subproject, generator_opts, rules, build_lines, link_targets)?;
		}

		for lib in &project.static_libraries {
			if !link_targets.contains_key(&LinkPtr::Static(lib.clone())) {
				add_static_lib_target(lib, generator_opts, rules, build_lines, link_targets)?;
			}
		}

		for lib in &project.object_libraries {
			if !link_targets.contains_key(&LinkPtr::Object(lib.clone())) {
				add_object_lib_target(lib, generator_opts, rules, build_lines, link_targets)?;
			}
		}

		for lib in &project.interface_libraries {
			let key = LinkPtr::Interface(lib.clone());
			link_targets.entry(key).or_default();
		}

		for exe in &project.executables {
			add_executable_target(exe, generator_opts, rules, build_lines, link_targets)?;
		}
		Ok(())
	}
}

fn add_static_lib_target(
	lib: &Arc<StaticLibrary>,
	generator_opts: &GeneratorOpts,
	rules: &mut NinjaRules,
	build_lines: &mut Vec<NinjaBuild>,
	link_targets: &mut HashMap<LinkPtr, Vec<String>>,
) -> Result<Vec<String>, String> {
	let GeneratorOpts { toolchain, build_dir, target_platform, star_context, .. } = generator_opts;
	let mut inputs = Vec::<String>::new();

	let generator_vars = if let Some(gen_func) = &lib.generator_vars {
		eval_vars(gen_func, star_context.clone(), "generator_vars")?
	} else {
		StarGeneratorVars::default()
	};
	let mut includes = lib.public_includes_recursive();
	includes.extend_from_slice(&lib.private_includes());
	includes.extend(
		generator_vars
			.include_dirs
			.iter()
			.map(|x| join_parent(&lib.project().info.path, x).full),
	);
	let sources = lib
		.sources
		.extended_with(Sources::from_slice(&generator_vars.sources, &lib.project().info.path)?);
	let mut defines = lib.public_defines_recursive();
	defines.extend_from_slice(lib.private_defines());
	defines.extend_from_slice(&generator_vars.defines);

	let source_data = SourceData { includes, defines };

	add_obj_sources(&sources, generator_opts, lib.as_ref(), &source_data, rules, build_lines, &mut inputs)?;

	let out_name = output_path(build_dir, &lib.project().info.name, lib.output_name(), &target_platform.static_lib_ext);
	let output_targets = vec![out_name.clone()];
	let rule_name = match &rules.link_static_lib {
		Some(x) => x.name.clone(),
		None => {
			let static_linker = match &toolchain.static_linker {
				Some(x) => x,
				None => {
					return Err(format!(
						"No static linker specified in toolchain. A static linker is required to build \"{}\".",
						lib.name()
					))
				}
			};
			let link_static_lib_rule = link_static_lib(static_linker);
			let rule_name = link_static_lib_rule.name.clone();
			rules.link_static_lib = Some(link_static_lib_rule);
			rule_name
		}
	};
	let link_flags = Vec::new();
	build_lines.push(NinjaBuild {
		inputs,
		output_targets: output_targets.clone(),
		rule_name,
		keyval_set: HashMap::from([
			("TARGET_FILE".to_string(), vec![out_name.clone()]),
			("LINK_FLAGS".to_string(), link_flags),
		]),
	});
	build_lines.push(NinjaBuild {
		inputs: vec![out_name],
		output_targets: vec![lib.name.clone()],
		rule_name: "phony".to_owned(),
		keyval_set: HashMap::new(),
	});
	link_targets.insert(LinkPtr::Static(lib.clone()), output_targets.clone());
	Ok(output_targets)
}

fn add_object_lib_target(
	lib: &Arc<ObjectLibrary>,
	generator_opts: &GeneratorOpts,
	rules: &mut NinjaRules,
	build_lines: &mut Vec<NinjaBuild>,
	link_targets: &mut HashMap<LinkPtr, Vec<String>>,
) -> Result<Vec<String>, String> {
	let GeneratorOpts { build_dir, target_platform, star_context, .. } = generator_opts;
	let mut inputs = Vec::<String>::new();

	let generator_vars = if let Some(gen_func) = &lib.generator_vars {
		eval_vars(gen_func, star_context.clone(), "generator_vars")?
	} else {
		StarGeneratorVars::default()
	};
	let mut includes = lib.public_includes_recursive();
	includes.extend_from_slice(&lib.private_includes());
	includes.extend(
		generator_vars
			.include_dirs
			.iter()
			.map(|x| join_parent(&lib.project().info.path, x).full),
	);
	let sources = lib
		.sources
		.extended_with(Sources::from_slice(&generator_vars.sources, &lib.project().info.path)?);
	let mut defines = lib.public_defines_recursive();
	defines.extend_from_slice(lib.private_defines());
	defines.extend_from_slice(&generator_vars.defines);

	let source_data = SourceData { includes, defines };

	add_obj_sources(&sources, generator_opts, lib.as_ref(), &source_data, rules, build_lines, &mut inputs)?;

	for link in &lib.public_links_recursive() {
		match link {
			LinkPtr::Static(_) => {
				let link_path = output_path(
					build_dir,
					&link.project().info.name,
					link.output_name(),
					&target_platform.static_lib_ext,
				);
				if !inputs.contains(&link_path) {
					inputs.push(link_path);
				}
			}
			LinkPtr::Object(_) => {}
			LinkPtr::Interface(_) => {}
		}
	}
	link_targets.insert(LinkPtr::Object(lib.clone()), inputs.clone());
	Ok(inputs)
	// Omit phony rules for object libraries
}

fn add_executable_target(
	exe: &Arc<Executable>,
	generator_opts: &GeneratorOpts,
	rules: &mut NinjaRules,
	build_lines: &mut Vec<NinjaBuild>,
	link_targets: &mut HashMap<LinkPtr, Vec<String>>,
) -> Result<(), String> {
	let GeneratorOpts {
		toolchain,
		build_dir,
		profile,
		global_opts,
		target_platform,
		star_context,
		..
	} = generator_opts;

	log::debug!("   exe target: {}", exe.name);
	let mut inputs = Vec::<String>::new();

	let generator_vars = if let Some(gen_func) = &exe.generator_vars {
		eval_vars(gen_func, star_context.clone(), "generator_vars")?
	} else {
		StarGeneratorVars::default()
	};
	let mut includes = exe.public_includes_recursive();
	includes.extend(
		generator_vars
			.include_dirs
			.iter()
			.map(|x| join_parent(&exe.project().info.path, x).full),
	);
	let sources = exe
		.sources
		.extended_with(Sources::from_slice(&generator_vars.sources, &exe.project().info.path)?);
	let mut defines = exe.public_defines_recursive();
	defines.extend_from_slice(&generator_vars.defines);

	let source_data = SourceData { includes, defines };

	if !sources.c.is_empty() {
		let c_compiler = get_c_compiler(toolchain, exe.name())?;
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
		for src in &sources.c {
			build_lines.push(add_obj_source(
				input_path(&src.full, &exe.project().info.path),
				&source_data,
				output_subfolder_path(
					build_dir,
					&exe.project().info.name,
					&exe.name,
					&src.name,
					&target_platform.obj_ext,
				),
				rule_compile_c.name.clone(),
				c_compile_opts.clone(),
				&mut inputs,
			));
		}
	}
	if !sources.cpp.is_empty() {
		let cpp_compiler = get_cpp_compiler(toolchain, exe.name())?;
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
		for src in &sources.cpp {
			build_lines.push(add_obj_source(
				input_path(&src.full, &exe.project().info.path),
				&source_data,
				output_subfolder_path(
					build_dir,
					&exe.project().info.name,
					&exe.name,
					&src.name,
					&target_platform.obj_ext,
				),
				rule_compile_cpp.name.clone(),
				cpp_compile_opts.clone(),
				&mut inputs,
			));
		}
	}
	if !sources.nasm.is_empty() {
		let nasm_assembler = get_nasm_assembler(toolchain, exe.name())?;
		let rule = if let Some(rule) = &rules.assemble_nasm_object {
			rule
		} else {
			rules.assemble_nasm_object = Some(assemble_nasm_object(nasm_assembler));
			rules.assemble_nasm_object.as_ref().unwrap()
		};
		let nasm_assemble_opts = &profile.nasm_assemble_flags;
		for src in &sources.nasm {
			build_lines.push(add_obj_source(
				input_path(&src.full, &exe.project().info.path),
				&source_data,
				output_subfolder_path(
					build_dir,
					&exe.project().info.name,
					&exe.name,
					&src.name,
					&target_platform.obj_ext,
				),
				rule.name.clone(),
				nasm_assemble_opts.clone(),
				&mut inputs,
			));
		}
	}
	for link in &exe.links {
		let link_outputs = match link_targets.get(link) {
			Some(x) => x,
			None => return Err(format!("Output target not found: {}", link.name())),
		};
		inputs.extend_from_slice(link_outputs);

		for translink in &link.public_links_recursive() {
			let link_outputs = match link_targets.get(translink) {
				Some(x) => x,
				None => return Err(format!("Transitive output target not found: {}", translink.name())),
			};
			inputs.extend_from_slice(link_outputs);
		}
	}
	// Prevent the same lib from being added to the command more than once.
	let inputs = deduplicate(inputs);
	let rule_name = match &rules.link_exe {
		Some(x) => x.name.clone(),
		None => {
			let exe_linker = match &toolchain.exe_linker {
				Some(x) => x,
				None => {
					return Err(format!(
						"No executable linker specified in toolchain. An executable linker is required to build \"{}\".",
						exe.name()
					))
				}
			};
			let exe_link_rule = link_exe(exe_linker.as_ref());
			let rule_name = exe_link_rule.name.clone();
			rules.link_exe = Some(exe_link_rule);
			rule_name
		}
	};
	let mut link_exe_flags = Vec::new();
	if let Some(true) = global_opts.position_independent_code {
		if let Some(pie_flag) = toolchain
			.exe_linker
			.as_ref()
			.unwrap()
			.position_independent_executable_flag()
		{
			link_exe_flags.push(pie_flag);
		}
	}
	let mut link_flags = link_exe_flags.clone();
	link_flags.extend(exe.link_flags_recursive());
	let out_name = output_path(build_dir, &exe.project().info.name, exe.name.as_ref(), &target_platform.exe_ext);
	build_lines.push(NinjaBuild {
		inputs,
		output_targets: vec![out_name.clone()],
		rule_name,
		keyval_set: HashMap::from([
			("TARGET_FILE".to_string(), vec![out_name.clone()]),
			("LINK_FLAGS".to_string(), link_flags),
		]),
	});
	build_lines.push(NinjaBuild {
		inputs: vec![out_name],
		output_targets: vec![exe.name.clone()],
		rule_name: "phony".to_owned(),
		keyval_set: HashMap::new(),
	});
	Ok(())
}

fn add_obj_sources(
	sources: &Sources,
	generator_opts: &GeneratorOpts,
	target: &dyn Target,
	source_data: &SourceData,
	rules: &mut NinjaRules,
	build_lines: &mut Vec<NinjaBuild>,
	inputs: &mut Vec<String>,
) -> Result<(), String> {
	let GeneratorOpts {
		toolchain, build_dir, profile, global_opts, target_platform, ..
	} = generator_opts;

	if !sources.c.is_empty() {
		let c_compiler = get_c_compiler(toolchain, target.name())?;
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
			if let Some(fpic_flag) = c_compiler.position_independent_code_flag() {
				c_compile_opts.push(fpic_flag);
			}
		}
		for src in &sources.c {
			build_lines.push(add_obj_source(
				input_path(&src.full, &target.project().info.path),
				source_data,
				output_subfolder_path(
					build_dir,
					&target.project().info.name,
					target.name(),
					&src.name,
					&target_platform.obj_ext,
				),
				rule_compile_c.name.clone(),
				c_compile_opts.clone(),
				inputs,
			));
		}
	}
	if !sources.cpp.is_empty() {
		let cpp_compiler = get_cpp_compiler(toolchain, target.name())?;
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
			if let Some(fpic_flag) = cpp_compiler.position_independent_code_flag() {
				cpp_compile_opts.push(fpic_flag);
			}
		}
		for src in &sources.cpp {
			build_lines.push(add_obj_source(
				input_path(&src.full, &target.project().info.path),
				source_data,
				output_subfolder_path(
					build_dir,
					&target.project().info.name,
					target.name(),
					&src.name,
					&target_platform.obj_ext,
				),
				rule_compile_cpp.name.clone(),
				cpp_compile_opts.clone(),
				inputs,
			));
		}
	}
	if !sources.nasm.is_empty() {
		let nasm_assembler = get_nasm_assembler(toolchain, target.name())?;
		let rule = if let Some(rule) = &rules.assemble_nasm_object {
			rule
		} else {
			rules.assemble_nasm_object = Some(assemble_nasm_object(nasm_assembler));
			rules.assemble_nasm_object.as_ref().unwrap()
		};
		let nasm_assemble_opts = &profile.nasm_assemble_flags;
		for src in &sources.nasm {
			build_lines.push(add_obj_source(
				input_path(&src.full, &target.project().info.path),
				source_data,
				output_subfolder_path(
					build_dir,
					&target.project().info.name,
					target.name(),
					&src.name,
					&target_platform.obj_ext,
				),
				rule.name.clone(),
				nasm_assemble_opts.clone(),
				inputs,
			));
		}
	}
	Ok(())
}

fn add_obj_source(
	input: String,
	source_data: &SourceData,
	out_tgt: String,
	rule_name: String,
	compile_options: Vec<String>,
	inputs: &mut Vec<String>,
) -> NinjaBuild {
	log::debug!("Ninja::add_obj_source() {out_tgt}");
	inputs.push(out_tgt.clone());
	NinjaBuild {
		inputs: vec![input],
		output_targets: vec![out_tgt.clone()],
		rule_name,
		keyval_set: HashMap::from([
			("DEFINES".to_string(), transform_defines(&source_data.defines)),
			("FLAGS".to_string(), compile_options),
			(
				"INCLUDES".to_owned(),
				source_data
					.includes
					.iter()
					.map(|x| "-I".to_owned() + x.to_string_lossy().trim_start_matches(r"\\?\"))
					.collect(),
			),
			("DEP_FILE".to_owned(), vec![out_tgt + ".d"]),
		]),
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

fn get_nasm_assembler<'a>(toolchain: &'a Toolchain, name: &str) -> Result<&'a dyn Assembler, String> {
	match toolchain.nasm_assembler {
		Some(ref x) => Ok(x.as_ref()),
		None => Err(format!(
			"No NASM assembler specified in toolchain. A NASM assembler is required to build NASM sources in \"{}\".",
			name
		)),
	}
}

fn deduplicate<T: Clone + Eq + Hash>(mut inputs: Vec<T>) -> Vec<T> {
	let mut unique_inputs: HashSet<T> = HashSet::new();
	inputs.retain(|x| unique_inputs.insert(x.clone()));
	inputs
}

#[test]
fn test_position_independent_code() {
	use crate::misc::{SourcePath, Sources};
	use core::default::Default;
	use std::path::PathBuf;

	struct TestAssembler {}
	impl Assembler for TestAssembler {
		fn id(&self) -> String {
			"nasm".to_owned()
		}
		fn version(&self) -> String {
			"2.16.0".to_owned()
		}
		fn cmd(&self) -> Vec<String> {
			vec!["nasm".to_owned()]
		}
		fn out_flag(&self) -> String {
			"-o".to_owned()
		}
		fn depfile_flags(&self, out_file: &str, dep_file: &str) -> Vec<String> {
			vec![
				"-MD".to_owned(),
				dep_file.to_owned(),
				"-MT".to_owned(),
				out_file.to_owned(),
			]
		}
	}

	struct TestCompiler {}
	impl Compiler for TestCompiler {
		fn id(&self) -> String {
			"clang".to_owned()
		}
		fn version(&self) -> String {
			"17.0.0".to_owned()
		}
		fn target(&self) -> String {
			"x86_64-unknown-linux-gnu".to_owned()
		}
		fn cmd(&self) -> Vec<String> {
			vec!["clang".to_owned()]
		}
		fn out_flag(&self) -> String {
			"-o".to_owned()
		}
		fn depfile_flags(&self, out_file: &str, dep_file: &str) -> Vec<String> {
			vec![
				"-MD".to_owned(),
				"-MT".to_owned(),
				out_file.to_owned(),
				"-MF".to_owned(),
				dep_file.to_owned(),
			]
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
					defines_private: Vec::new(),
					defines_public: Vec::new(),
					link_flags_public: Vec::new(),
					generator_vars: None,
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
			generator_vars: None,
			output_name: None,
		})],
		static_libraries: vec![create_lib(weak_parent)],
		object_libraries: Vec::new(),
		interface_libraries: Vec::new(),
	});
	let toolchain = Toolchain {
		msvc_platforms: vec!["x64".to_owned(), "Win32".to_owned(), "ARM64".to_owned()],
		c_compiler: Some(Box::new(TestCompiler {})),
		cpp_compiler: Some(Box::new(TestCompiler {})),
		nasm_assembler: Some(Box::new(TestAssembler {})),
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
	let generator_opts = GeneratorOpts {
		build_dir: PathBuf::from("build"),
		profile,
		global_opts,
		target_platform,
		toolchain,
		star_context: StarContext { c_compiler: None, cpp_compiler: None },
	};
	let mut link_targets = HashMap::new();
	let result = Ninja::generate_inner(&project, &generator_opts, &mut rules, &mut build_lines, &mut link_targets);

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
		.filter(|x| x.output_targets.first().unwrap() == &main_out_path)
		.collect::<Vec<_>>();
	assert_eq!(main_exe_rules.len(), 1);

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
