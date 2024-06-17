mod index_map;

use std::{
	collections::BTreeMap,
	fs,
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use uuid::Uuid;

use crate::{
	link_type::LinkPtr,
	misc::Sources,
	object_library::ObjectLibrary,
	project::{Project, ProjectInfo},
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
	toolchain::{Toolchain, VcxprojProfile},
	GlobalOptions,
};

use index_map::IndexMap;

const VS_CPP_GUID: &str = "8BC9CEB8-8B4A-11D0-8D11-00A0C91BC942";

#[derive(Clone)]
struct VsProject {
	name: String,
	guid: String,
	vcxproj_path: String,
	dependencies: Vec<VsProject>,
	has_nasm: bool,
}

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

enum CStd {
	C11,
	C17,
}

impl CStd {
	fn as_str(&self) -> &str {
		match self {
			CStd::C11 => "stdc11",
			CStd::C17 => "stdc17",
		}
	}
}

enum CppStd {
	Cpp11,
	Cpp14,
	Cpp17,
	Cpp20,
}

impl CppStd {
	fn as_str(&self) -> &str {
		match self {
			CppStd::Cpp11 => "stdcpp11",
			CppStd::Cpp14 => "stdcpp14",
			CppStd::Cpp17 => "stdcpp17",
			CppStd::Cpp20 => "stdcpp20",
		}
	}
}

struct Options {
	c_standard: Option<CStd>,
	cpp_standard: Option<CppStd>,
}

impl VsProject {
	fn to_sln_project_section(&self) -> String {
		let proj_name = &self.name;
		let vcxproj_path = &self.vcxproj_path;
		let guid = self.guid.to_string().to_ascii_uppercase();
		let mut ret = format!(
			r#"Project("{{{VS_CPP_GUID}}}") = "{proj_name}", "{vcxproj_path}", "{{{guid}}}"
"#
		);
		if !self.dependencies.is_empty() {
			ret += "	ProjectSection(ProjectDependencies) = postProject\n";
		}
		for dep in &self.dependencies {
			let dep_guid = &dep.guid.to_string().to_ascii_uppercase();
			ret += &format!("		{{{dep_guid}}} = {{{dep_guid}}}\n");
		}
		if !self.dependencies.is_empty() {
			ret += "	EndProjectSection\n";
		}
		ret += "EndProject\n";
		ret
	}
}

struct ProfileFragment {
	vcxproj: VcxprojProfile,
	nasm_assemble_flags: Vec<String>,
}

fn item_definition_group(
	platform: &str,
	profile_name: &str,
	profile: &ProfileFragment,
	sources: &Sources,
	include_dirs: &[String],
	defines: &[String],
	opts: &Options,
) -> Result<String, String> {
	let mut ret = format!(
		r#"  <ItemDefinitionGroup Condition="'$(Configuration)|$(Platform)'=='{profile_name}|{platform}'">
"#
	);

	if !sources.c.is_empty() || !sources.cpp.is_empty() {
		ret += &cl_compile(&profile.vcxproj, include_dirs, defines, opts, sources.cpp.is_empty());
	}
	if !sources.nasm.is_empty() {
		ret += &nasm_compile(profile, platform, include_dirs, defines)?;
	}
	if !profile.vcxproj.link.is_empty() {
		ret += "    <Link>\n";
		for (key, val) in &profile.vcxproj.link {
			ret += &format!("      <{key}>{val}</{key}>\n")
		}
		ret += "    </Link>\n";
	}
	ret += "  </ItemDefinitionGroup>\n";

	Ok(ret)
}

fn cl_compile(
	profile: &VcxprojProfile,
	include_dirs: &[String],
	defines: &[String],
	opts: &Options,
	compile_as_c: bool,
) -> String {
	let mut ret = "    <ClCompile>\n".to_owned();

	for (key, val) in &profile.cl_compile {
		ret += &format!("      <{key}>{val}</{key}>\n");
	}

	if compile_as_c {
		if let Some(c_std) = &opts.c_standard {
			ret += "      <LanguageStandard_C>";
			ret += c_std.as_str();
			ret += "</LanguageStandard_C>\n";
			ret += "      <CompileAs>CompileAsC</CompileAs>\n";
		}
	} else if let Some(cpp_std) = &opts.cpp_standard {
		ret += "      <LanguageStandard>";
		ret += cpp_std.as_str();
		ret += "</LanguageStandard>\n";
	}

	ret += "      <AdditionalIncludeDirectories>";
	ret += &include_dirs
		.iter()
		.chain(&["%(AdditionalIncludeDirectories)".to_owned()])
		.fold(String::new(), |acc, x| acc + ";" + x);
	ret += "</AdditionalIncludeDirectories>\n";

	ret += "      <ConformanceMode>true</ConformanceMode>\n";

	// TODO(Travers): Add global options for warnings
	// <WarningLevel>Level4</WarningLevel>
	// <TreatWarningAsError>false</TreatWarningAsError>
	// TODO(Travers): Add other definitions and compile flags
	ret += "      <PreprocessorDefinitions>";
	ret += &profile
		.preprocessor_definitions
		.iter()
		.chain(defines)
		.chain([&"%(PreprocessorDefinitions)".to_owned()])
		.fold(String::new(), |acc, x| acc + x + ";");
	ret += "</PreprocessorDefinitions>\n";
	// ret += r#"      <ObjectFileName>$(IntDir)</ObjectFileName>
	ret += "    </ClCompile>\n";
	ret
}

fn nasm_compile(
	profile: &ProfileFragment,
	platform: &str,
	include_dirs: &[String],
	defines: &[String],
) -> Result<String, String> {
	let mut ret = "    <NASM>\n".to_owned();

	ret += "      <Format>";
	ret += map_platform_to_nasm_format(platform)?;
	ret += "</Format>\n";

	ret += "      <IncludePaths>";
	ret += &include_dirs.join(";");
	ret += "</IncludePaths>\n";

	ret += "      <Define>";
	ret += &profile
		.vcxproj
		.preprocessor_definitions
		.iter()
		.chain(defines)
		.fold(String::new(), |acc, x| acc + x + ";");
	ret += "%(PreprocessorDefinitions)</Define>\n"; // TODO(Travers): Check this

	ret += "      <AdditionalOptions>";
	ret += &profile.nasm_assemble_flags.join(" ");
	ret += "</AdditionalOptions>\n";

	ret += "    </NASM>\n";
	Ok(ret)
}

struct TargetData {
	name: String,
	includes: Vec<String>,
	defines: Vec<String>,
	links: Vec<LinkPtr>,
}

struct VcxprojOpts {
	build_dir: PathBuf,
	profiles: BTreeMap<String, ProfileFragment>,
	msvc_platforms: Vec<String>,
	opts: Options,
}

pub struct Msvc {}

impl Msvc {
	pub fn generate(
		project: Arc<Project>,
		build_dir: &Path,
		toolchain: Toolchain,
		global_opts: GlobalOptions,
	) -> Result<(), String> {
		if toolchain.msvc_platforms.is_empty() {
			return Err("Toolchain doesn't contain any msvc_platforms, required for MSVC generator".to_owned());
		}
		let mut guid_map = IndexMap::new();
		let c_standard = match global_opts.c_standard {
			None => None,
			Some(x) => match x.as_str() {
				"11" => Some(CStd::C11),
				"17" => Some(CStd::C17),
				_ => {
					return Err(format!(
						"Unrecognized value for option for \"c_standard\": \"{x}\". Accepted values are \"17\", \"11\"",
					))
				}
			},
		};
		let cpp_standard = match global_opts.cpp_standard {
			None => None,
			Some(x) => match x.as_str() {
				"11" => Some(CppStd::Cpp11),
				"14" => Some(CppStd::Cpp14),
				"17" => Some(CppStd::Cpp17),
				"20" => Some(CppStd::Cpp20),
				_ => {
					return Err(format!(
						"Unrecognized value for option for \"cpp_standard\": \"{x}\". Accepted values are \"20\", \"17\", \"14\", \"11\"",
					))
				}
			},
		};
		let profiles = toolchain
			.profile
			.iter()
			.filter_map(|x| {
				x.1.vcxproj.as_ref().map(|prof| {
					(
						x.0.clone(),
						ProfileFragment {
							vcxproj: prof.clone(),
							nasm_assemble_flags: x.1.nasm_assemble_flags.clone(),
						},
					)
				})
			})
			.collect::<BTreeMap<String, ProfileFragment>>();
		if profiles.is_empty() {
			return Err(
				"Toolchain doesn't contain any profiles with a \"vcxproj\" section, required for MSVC generator"
					.to_owned(),
			);
		}
		let proj_opts = VcxprojOpts {
			build_dir: build_dir.to_owned(),
			profiles,
			msvc_platforms: toolchain.msvc_platforms,
			opts: Options { c_standard, cpp_standard },
		};
		Self::generate_inner(&project, &proj_opts, &mut guid_map)?;

		let mut sln_content = r#"Microsoft Visual Studio Solution File, Format Version 12.00
"#
		.to_string();

		// Reverse iterate to put the most important projects at the top of the Solution Explorer
		for proj in guid_map.iter().rev() {
			sln_content += &proj.to_sln_project_section();
		}
		sln_content += r#"Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
"#;
		for profile in &toolchain.profile {
			let profile_name = profile.0;
			for platform in &proj_opts.msvc_platforms {
				sln_content += &format!("\t\t{profile_name}|{platform} = {profile_name}|{platform}\n");
			}
		}
		sln_content += "	EndGlobalSection\n";

		sln_content += "	GlobalSection(ProjectConfigurationPlatforms) = postSolution\n";
		for proj in &guid_map {
			let guid = &proj.guid.to_string().to_ascii_uppercase();
			for profile in &toolchain.profile {
				let prof_name = profile.0;
				for platform in &proj_opts.msvc_platforms {
					sln_content += &format!("		{{{guid}}}.{prof_name}|{platform}.ActiveCfg = {prof_name}|{platform}\n");
					sln_content += &format!("		{{{guid}}}.{prof_name}|{platform}.Build.0 = {prof_name}|{platform}\n");
				}
			}
		}
		sln_content += "	EndGlobalSection\n";

		let sln_guid = Uuid::new_v4().to_string().to_ascii_uppercase();
		sln_content += &format!(
			r#"	GlobalSection(SolutionProperties) = preSolution
		HideSolutionNode = FALSE
	EndGlobalSection
	GlobalSection(ExtensibilityGlobals) = postSolution
		SolutionGuid = {{{sln_guid}}}
	EndGlobalSection
"#
		);
		sln_content += "EndGlobal\n";

		let sln_pathbuf = build_dir.join(project.info.name.clone() + ".sln");
		write_file(&sln_pathbuf, &sln_content)?;

		if guid_map.iter().any(|x| x.has_nasm) {
			if let Some(nasm_assembler) = toolchain.nasm_assembler {
				write_file(&build_dir.join("nasm.xml"), NASM_XML_CONTENT)?;
				write_file(&build_dir.join("nasm.props"), &nasm_props_content(&nasm_assembler.cmd()))?;
				write_file(&build_dir.join("nasm.targets"), NASM_TARGETS_CONTENT)?;
			} else {
				return Err(
					"Toolchain does not contain a NASM assembler, required for files in this project".to_owned()
				);
			}
		}

		Ok(())
	}

	fn generate_inner(project: &Arc<Project>, proj_opts: &VcxprojOpts, guid_map: &mut IndexMap) -> Result<(), String> {
		for subproject in &project.dependencies {
			Self::generate_inner(subproject, proj_opts, guid_map)?;
		}

		for lib in &project.static_libraries {
			if !guid_map.contains_key(&LinkPtr::Static(lib.clone())) {
				add_static_lib(lib, proj_opts, guid_map)?;
			}
		}
		for lib in &project.object_libraries {
			if !guid_map.contains_key(&LinkPtr::Object(lib.clone())) {
				add_object_lib(lib, proj_opts, guid_map)?;
			}
		}
		for exe in &project.executables {
			let configuration_type = "Application";
			let project_info = &exe.project().info;
			let target_flags = TargetData {
				name: exe.name.clone(),
				// Visual Studio doesn't seem to support extended-length name syntax
				includes: exe
					.public_includes_recursive()
					.into_iter()
					.map(|x| x.to_string_lossy().trim_start_matches(r"\\?\").to_owned())
					.collect::<Vec<String>>(),
				defines: exe.public_defines_recursive(),
				links: exe.links.clone(),
			};
			let vsproj =
				make_vcxproj(proj_opts, guid_map, configuration_type, project_info, &target_flags, &exe.sources)?;
			guid_map.insert_exe(vsproj);
		}
		Ok(())
	}
}

fn add_static_lib(
	lib: &Arc<StaticLibrary>,
	proj_opts: &VcxprojOpts,
	guid_map: &mut IndexMap,
) -> Result<VsProject, String> {
	log::debug!("add_static_lib: {}", lib.name);
	let project_info = &lib.project().info;
	let mut includes = lib.public_includes_recursive();
	includes.extend_from_slice(&lib.private_includes());
	let includes = includes
		.into_iter()
		// Visual Studio doesn't seem to support extended-length name syntax
		.map(|x| x.to_string_lossy().trim_start_matches(r"\\?\").to_owned())
		.collect::<Vec<String>>();
	let mut defines = lib.public_defines_recursive();
	defines.extend_from_slice(lib.private_defines());
	let links = lib
		.link_private
		.iter()
		.cloned()
		.chain(lib.link_public.iter().cloned())
		.collect();
	let target_flags = TargetData { name: lib.name.clone(), includes, defines, links };
	let vsproj = make_vcxproj(proj_opts, guid_map, "StaticLibrary", project_info, &target_flags, &lib.sources)?;
	let link_ptr = LinkPtr::Static(lib.clone());
	guid_map.insert(link_ptr, vsproj.clone());
	Ok(vsproj)
}

fn add_object_lib(
	lib: &Arc<ObjectLibrary>,
	proj_opts: &VcxprojOpts,
	guid_map: &mut IndexMap,
) -> Result<VsProject, String> {
	log::debug!("add_object_lib: {}", lib.name);
	let project_info = &lib.project().info;
	let mut includes = lib.public_includes_recursive();
	includes.extend_from_slice(&lib.private_includes());
	let includes = includes
		.into_iter()
		// Visual Studio doesn't seem to support extended-length name syntax
		.map(|x| x.to_string_lossy().trim_start_matches(r"\\?\").to_owned())
		.collect::<Vec<String>>();
	let mut defines = lib.public_defines_recursive();
	defines.extend_from_slice(lib.private_defines());
	let links = lib
		.link_private
		.iter()
		.cloned()
		.chain(lib.link_public.iter().cloned())
		.collect();
	let target_data = TargetData { name: lib.name.clone(), includes, defines, links };
	let vsproj = make_vcxproj(proj_opts, guid_map, "StaticLibrary", project_info, &target_data, &lib.sources)?;
	guid_map.insert(LinkPtr::Object(lib.clone()), vsproj.clone());
	Ok(vsproj)
}

fn make_vcxproj(
	proj_opts: &VcxprojOpts,
	guid_map: &mut IndexMap,
	configuration_type: &str,
	project_info: &ProjectInfo,
	target_data: &TargetData,
	sources: &Sources,
) -> Result<VsProject, String> {
	let target_name = &target_data.name;
	log::debug!("make_vcxproj: {target_name}");
	if !sources.c.is_empty() && !sources.cpp.is_empty() {
		return Err(format!("This generator does not support mixing C and C++ sources. Consider splitting them into separate libraries. Target: {target_name}"));
	}
	const PLATFORM_TOOLSET: &str = "v143";
	let target_guid = Uuid::new_v4().to_string().to_ascii_uppercase();
	let mut out_str = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemGroup Label="ProjectConfigurations">
"#
	.to_owned();
	for platform in &proj_opts.msvc_platforms {
		for profile_name in proj_opts.profiles.keys() {
			out_str += &format!(
				r#"    <ProjectConfiguration Include="{profile_name}|{platform}">
      <Configuration>{profile_name}</Configuration>
      <Platform>{platform}</Platform>
    </ProjectConfiguration>
"#
			);
		}
	}
	out_str += "  </ItemGroup>\n";
	out_str += &format!(
		r#"  <PropertyGroup Label="Globals">
    <VCProjectVersion>16.0</VCProjectVersion>
    <Keyword>Win32Proj</Keyword>
    <ProjectGuid>{{{target_guid}}}</ProjectGuid>
    <RootNamespace>{target_name}</RootNamespace>
    <WindowsTargetPlatformVersion>10.0</WindowsTargetPlatformVersion>
  </PropertyGroup>
  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.default.props" />
"#
	);
	for platform in &proj_opts.msvc_platforms {
		for (profile_name, profile_cfg) in &proj_opts.profiles {
			out_str += &format!(
				r#"  <PropertyGroup Condition="'$(Configuration)|$(Platform)'=='{profile_name}|{platform}'" Label="Configuration">
    <ConfigurationType>{configuration_type}</ConfigurationType>
    <PlatformToolset>{PLATFORM_TOOLSET}</PlatformToolset>
"#
			);
			// <UseDebugLibraries>true</UseDebugLibraries>
			// <CharacterSet>MultiByte</CharacterSet>
			// <WholeProgramOptimization>true</WholeProgramOptimization>
			for (prop_name, prop_val) in &profile_cfg.vcxproj.property_group {
				out_str += &format!("    <{prop_name}>{prop_val}</{prop_name}>\n");
			}
			out_str += "  </PropertyGroup>\n";
		}
	}
	out_str += r#"  <Import Project="$(VCTargetsPath)\\Microsoft.Cpp.props" />
  <ImportGroup Label="ExtensionSettings">
"#;

	let mut item_definition_groups = Vec::new();
	for platform in &proj_opts.msvc_platforms {
		for (profile_name, profile) in &proj_opts.profiles {
			item_definition_groups.push(item_definition_group(
				platform,
				profile_name,
				profile,
				sources,
				&target_data.includes,
				&target_data.defines,
				&proj_opts.opts,
			)?);
		}
	}
	let item_definition_groups = item_definition_groups;

	if !sources.nasm.is_empty() {
		out_str += r#"    <Import Project="..\..\nasm.props" />
"#;
	}
	out_str += r#"  </ImportGroup>
  <ImportGroup Label="Shared">
  </ImportGroup>
"#;
	for platform in &proj_opts.msvc_platforms {
		for profile_name in proj_opts.profiles.keys() {
			out_str += &format!(
				r#"  <ImportGroup Label="PropertySheets" Condition="'$(Configuration)|$(Platform)'=='{profile_name}|{platform}'">
    <Import Project="$(UserRootDir)\Microsoft.Cpp.$(Platform).user.props" Condition="exists('$(UserRootDir)\Microsoft.Cpp.$(Platform).user.props')" Label="LocalAppDataPlatform" />
  </ImportGroup>
"#
			);
		}
	}

	out_str += "  <PropertyGroup Label=\"UserMacros\" />\n";

	for item in item_definition_groups {
		out_str += &item;
	}
	if !sources.c.is_empty() {
		out_str += "  <ItemGroup>\n";
		for src in &sources.c {
			let input = input_path(&src.full, &project_info.path);
			out_str += &format!("    <ClCompile Include=\"{input}\" />\n");
		}
		out_str += "  </ItemGroup>\n";
	}
	if !sources.cpp.is_empty() {
		out_str += "  <ItemGroup>\n";
		for src in &sources.cpp {
			let input = input_path(&src.full, &project_info.path);
			out_str += &format!("    <ClCompile Include=\"{input}\" />\n");
		}
		out_str += "  </ItemGroup>\n";
	}
	if !sources.nasm.is_empty() {
		out_str += "  <ItemGroup>\n";
		for src in &sources.nasm {
			let input = input_path(&src.full, &project_info.path);
			out_str += &format!("    <NASM Include=\"{input}\" />\n");
		}
		out_str += "  </ItemGroup>\n";
	}

	let mut dependencies = Vec::new();
	if !target_data.links.is_empty() {
		out_str += "  <ItemGroup>\n";
		out_str += &add_project_references(&target_data.links, proj_opts, guid_map, &mut dependencies)?;
		out_str += "  </ItemGroup>\n";
	}
	out_str += r#"  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.targets" />
  <ImportGroup Label="ExtensionTargets">
"#;
	if !sources.nasm.is_empty() {
		out_str += r#"    <Import Project="..\..\nasm.targets" />
"#;
	}
	out_str += "  </ImportGroup>\n";
	out_str += "</Project>\n";
	let vcxproj_pathbuf = PathBuf::from(&project_info.name)
		.join(target_name)
		.join(target_name.to_owned() + ".vcxproj");
	let vcxproj_pathbuf_abs = proj_opts.build_dir.join(&vcxproj_pathbuf);
	let vcxproj_path = vcxproj_pathbuf.to_string_lossy().into_owned();
	let vsproj = VsProject {
		name: target_name.to_owned(),
		guid: target_guid,
		vcxproj_path,
		dependencies,
		has_nasm: !sources.nasm.is_empty(),
	};

	if let Err(e) = fs::create_dir_all(vcxproj_pathbuf_abs.parent().unwrap()) {
		return Err(format!("Error creating directory for \"{}\": {}", vcxproj_pathbuf.to_string_lossy(), e));
	};
	write_file(&vcxproj_pathbuf_abs, &out_str)?;
	Ok(vsproj)
}

fn add_project_references(
	project_links: &Vec<LinkPtr>,
	proj_opts: &VcxprojOpts,
	guid_map: &mut IndexMap,
	dependencies: &mut Vec<VsProject>,
) -> Result<String, String> {
	log::debug!("add_project_references() {}", project_links.len());
	let mut out_str = String::new();
	for link in project_links {
		log::debug!("   link: {}", link.name());
		let mut add_dependency = |proj_ref: &VsProject| {
			log::debug!("   add_dependency() {}", proj_ref.name);
			dependencies.push(proj_ref.clone());
			let proj_ref_include = proj_opts.build_dir.join(&proj_ref.vcxproj_path);
			out_str += &format!(
				r#"    <ProjectReference Include="{}">
      <Project>{{{}}}</Project>
      <Name>{}</Name>
      <ReferenceOutputAssembly>false</ReferenceOutputAssembly>
      <CopyToOutputDirectory>Never</CopyToOutputDirectory>
    </ProjectReference>
"#,
				proj_ref_include.to_string_lossy(),
				proj_ref.guid,
				link.name()
			);
		};
		log::debug!("   match link: {}", link.name());
		match link {
			LinkPtr::Static(static_lib) => {
				let proj_ref = match guid_map.get(link) {
					Some(x) => x,
					None => {
						add_static_lib(static_lib, proj_opts, guid_map)?;
						guid_map.get(link).unwrap()
					}
				};
				add_dependency(proj_ref);
			}
			LinkPtr::Object(obj_lib) => {
				let proj_ref = match guid_map.get(link) {
					Some(x) => x,
					None => {
						add_object_lib(obj_lib, proj_opts, guid_map)?;
						guid_map.get(link).unwrap()
					}
				};
				add_dependency(proj_ref);
			}
			LinkPtr::Interface(_) => {
				out_str += &add_project_references(&link.public_links(), proj_opts, guid_map, dependencies)?;
			}
		}
	}
	Ok(out_str)
}

fn write_file(filepath: &Path, content: &str) -> Result<(), String> {
	let mut f = match fs::File::create(filepath) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error creating file at \"{}\": {}", filepath.to_string_lossy(), e)),
	};
	if let Err(e) = f.write_all(content.as_bytes()) {
		return Err(format!("Error writing to {}: {}", filepath.to_string_lossy(), e));
	}
	Ok(())
}

const NASM_XML_CONTENT: &str = include_str!("nasm.xml");
const NASM_TARGETS_CONTENT: &str = include_str!("nasm.targets");

fn nasm_props_content(nasm_cmd: &[String]) -> String {
	let mapped_vec = nasm_cmd.iter().map(|x| format!("\"{}\"", x)).collect::<Vec<String>>();
	format!(include_str!("nasm.props"), mapped_vec.join(" "))
}

fn map_platform_to_nasm_format(platform: &str) -> Result<&'static str, String> {
	let platform_target = [("Win32", "win32"), ("x64", "win64")];
	match platform_target.iter().find(|x| platform == x.0) {
		Some(x) => Ok(x.1),
		None => Err(format!(
			"Unknown platform: {platform}. Known platforms are {}",
			platform_target.map(|x| format!("\"{}\"", x.0)).join(", ")
		)),
	}
}
