use std::{
	collections::{BTreeMap, HashMap},
	fs,
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use uuid::Uuid;

use crate::{
	link_type::LinkPtr,
	misc::Sources,
	project::{Project, ProjectInfo},
	target::{LinkTarget, Target},
	toolchain::{Profile, VcxprojProfile},
	GlobalOptions,
};

const VS_CPP_GUID: &str = "8BC9CEB8-8B4A-11D0-8D11-00A0C91BC942";

#[derive(Clone)]
struct VsProject {
	name: String,
	guid: String,
	vcxproj_path: String,
	dependencies: Vec<VsProject>,
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

fn item_definition_group(
	profile_name: &str,
	profile: &VcxprojProfile,
	include_dirs: &[String],
	defines: &[String],
	// compile_flags: &[String],
	opts: &Options,
	compile_as_c: bool,
) -> String {
	let mut ret = format!(
		r#"  <ItemDefinitionGroup Condition="'$(Configuration)|$(Platform)'=='{profile_name}|{}'">
    <ClCompile>
"#,
		profile.platform
	);
	for (key, val) in &profile.cl_compile {
		ret += &format!("      <{key}>{val}</{key}>\n");
	}
	ret += "      <AdditionalIncludeDirectories>";
	ret += &include_dirs.join(";");
	ret += "%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>\n";
	ret += "      <ConformanceMode>true</ConformanceMode>\n";

	if let Some(c_std) = &opts.c_standard {
		ret += "      <LanguageStandard_C>";
		ret += c_std.as_str();
		ret += "</LanguageStandard_C>\n";
	}
	if let Some(cpp_std) = &opts.cpp_standard {
		ret += "      <LanguageStandard>";
		ret += cpp_std.as_str();
		ret += "</LanguageStandard>\n";
	}
	if compile_as_c {
		ret += "      <CompileAs>CompileAsC</CompileAs>\n";
	}
	// TODO(Travers): Add global options for warnings
	// <WarningLevel>Level4</WarningLevel>
	// <TreatWarningAsError>false</TreatWarningAsError>
	// TODO(Travers): Add other definitions and compile flags
	ret += "      <PreprocessorDefinitions>";
	for def in &profile.preprocessor_definitions {
		ret += def;
		ret += ";";
	}
	for def in defines {
		ret += def;
		ret += ";";
	}
	ret += "%(PreprocessorDefinitions)</PreprocessorDefinitions>\n";
	ret += r#"      <ObjectFileName>$(IntDir)</ObjectFileName>
    </ClCompile>
    <ResourceCompile>
      <PreprocessorDefinitions>%(PreprocessorDefinitions)"#;
	for def in &profile.preprocessor_definitions {
		ret += ";";
		ret += def;
	}
	for def in defines {
		ret += def;
		ret += ";";
	}
	ret += r#"</PreprocessorDefinitions>
      <AdditionalIncludeDirectories>"#;
	ret += &include_dirs.join(";");
	ret += r#"%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>
    </ResourceCompile>
    <Midl>
      <AdditionalIncludeDirectories>"#;
	ret += &include_dirs.join(";");
	ret += r#"%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>
      <OutputDirectory>$(ProjectDir)/$(IntDir)</OutputDirectory>
      <HeaderFileName>%(Filename).h</HeaderFileName>
      <TypeLibraryName>%(Filename).tlb</TypeLibraryName>
      <InterfaceIdentifierFileName>%(Filename)_i.c</InterfaceIdentifierFileName>
      <ProxyFileName>%(Filename)_p.c</ProxyFileName>
    </Midl>
    <Lib>
"#;
	ret +=
		&format!("      <AdditionalOptions>%(AdditionalOptions) /machine:{}</AdditionalOptions>\n", profile.platform);
	ret += r#"    </Lib>
  </ItemDefinitionGroup>
"#;

	ret
}

pub struct Msvc {}

impl Msvc {
	pub fn generate(
		project: Arc<Project>,
		build_dir: &Path,
		profiles: BTreeMap<String, Profile>,
		global_opts: GlobalOptions,
	) -> Result<(), String> {
		let mut guid_map = HashMap::<LinkPtr, VsProject>::new();
		let mut project_vec = Vec::new();
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
		let vcxproj_profiles = profiles
			.into_iter()
			.filter_map(|x| match x.1.vcxproj {
				None => None,
				Some(prof) => Some((x.0, prof)),
			})
			.collect::<BTreeMap<String, VcxprojProfile>>();
		if vcxproj_profiles.is_empty() {
			return Err(
				"Toolchain doesn't contain any profiles with a \"vcxproj\" section, required for MSVC generator"
					.to_owned(),
			);
		}
		let opts = Options { c_standard, cpp_standard };
		Self::generate_inner(&project, build_dir, &vcxproj_profiles, &mut guid_map, &mut project_vec, &opts)?;

		let mut sln_content = r#"Microsoft Visual Studio Solution File, Format Version 12.00
"#
		.to_string();

		// Reverse iterate to put the most important projects at the top of the Solution Explorer
		for proj in project_vec.iter().rev() {
			sln_content += &proj.to_sln_project_section();
		}
		sln_content += r#"Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|x64 = Debug|x64
		MinSizeRel|x64 = MinSizeRel|x64
		Release|x64 = Release|x64
		RelWithDebInfo|x64 = RelWithDebInfo|x64
	EndGlobalSection
"#;

		sln_content += "	GlobalSection(ProjectConfigurationPlatforms) = postSolution\n";
		for proj in &project_vec {
			let guid = &proj.guid.to_string().to_ascii_uppercase();
			sln_content += &format!(
				r#"		{{{guid}}}.Debug|x64.ActiveCfg = Debug|x64
		{{{guid}}}.Debug|x64.Build.0 = Debug|x64
		{{{guid}}}.MinSizeRel|x64.ActiveCfg = MinSizeRel|x64
		{{{guid}}}.MinSizeRel|x64.Build.0 = MinSizeRel|x64
		{{{guid}}}.Release|x64.ActiveCfg = Release|x64
		{{{guid}}}.Release|x64.Build.0 = Release|x64
		{{{guid}}}.RelWithDebInfo|x64.ActiveCfg = RelWithDebInfo|x64
		{{{guid}}}.RelWithDebInfo|x64.Build.0 = RelWithDebInfo|x64
"#
			);
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
		// GlobalSection(ExtensibilityAddIns) = postSolution
		// EndGlobalSection
		sln_content += "EndGlobal\n";

		let sln_pathbuf = build_dir.join(project.info.name.clone() + ".sln");

		if let Err(e) = fs::create_dir_all(sln_pathbuf.parent().unwrap()) {
			return Err(format!("Error creating directory for \"{}\": {}", sln_pathbuf.to_string_lossy(), e));
		};
		let mut f = match fs::File::create(sln_pathbuf.clone()) {
			Ok(x) => x,
			Err(e) => return Err(format!("Error creating sln at \"{}\": {}", sln_pathbuf.to_string_lossy(), e)),
		};
		if let Err(e) = f.write_all(sln_content.as_bytes()) {
			return Err(format!("Error writing to sln: {}", e));
		}

		Ok(())
	}
	fn generate_inner(
		project: &Arc<Project>,
		build_dir: &Path,
		profiles: &BTreeMap<String, VcxprojProfile>,
		guid_map: &mut HashMap<LinkPtr, VsProject>,
		project_vec: &mut Vec<VsProject>,
		opts: &Options,
	) -> Result<(), String> {
		for subproject in &project.dependencies {
			Self::generate_inner(subproject, build_dir, profiles, guid_map, project_vec, opts)?;
		}

		for lib in &project.static_libraries {
			let target_name = &lib.name;
			let configuration_type = "StaticLibrary";
			let target_ext = ".lib";
			let project_info = &lib.project().info;
			let mut includes = lib.public_includes_recursive();
			includes.extend_from_slice(&lib.private_includes());
			let includes = includes
				.into_iter()
				// Visual Studio doesn't seem to support extended-length name syntax
				// .map(|x| x.trim_start_matches(r"\\?\").to_owned())
				.map(|x| x.to_string_lossy().trim_start_matches(r"\\?\").to_owned())
				.collect::<Vec<String>>();
			let defines = lib.public_defines_recursive();
			let project_links = lib
				.link_private
				.iter()
				.cloned()
				.chain(lib.link_public.iter().cloned())
				.collect();
			let vsproj = make_vcxproj(
				build_dir,
				profiles,
				guid_map,
				target_name,
				configuration_type,
				target_ext,
				project_info,
				opts,
				&includes,
				&defines,
				&lib.sources,
				&project_links,
			)?;
			guid_map.insert(LinkPtr::Static(lib.clone()), vsproj.clone());
			project_vec.push(vsproj);
		}
		for exe in &project.executables {
			let target_name = &exe.name;
			let configuration_type = "Application";
			let target_ext = ".exe";
			let project_info = &exe.project().info;
			let includes = exe.public_includes_recursive();
			let defines = exe.public_defines_recursive();
			// Visual Studio doesn't seem to support extended-length name syntax
			let includes = includes
				.into_iter()
				.map(|x| x.to_string_lossy().trim_start_matches(r"\\?\").to_owned())
				.collect::<Vec<String>>();
			let vsproj = make_vcxproj(
				build_dir,
				profiles,
				guid_map,
				target_name,
				configuration_type,
				target_ext,
				project_info,
				opts,
				&includes,
				&defines,
				&exe.sources,
				&exe.links,
			)?;
			project_vec.push(vsproj);
		}
		Ok(())
	}
}

fn make_vcxproj(
	build_dir: &Path,
	profiles: &BTreeMap<String, VcxprojProfile>,
	guid_map: &HashMap<LinkPtr, VsProject>,
	target_name: &str,
	configuration_type: &str,
	target_ext: &str,
	project_info: &ProjectInfo,
	opts: &Options,
	includes: &[String],
	defines: &[String],
	sources: &Sources,
	project_links: &Vec<LinkPtr>,
) -> Result<VsProject, String> {
	if !sources.c.is_empty() && !sources.cpp.is_empty() {
		return Err(format!("This generator does not support mixing C and C++ sources. Consider splitting them into separate libraries. Target: {target_name}"));
	}
	const PLATFORM_TOOLSET: &str = "v143";
	let target_guid = Uuid::new_v4().to_string().to_ascii_uppercase();
	let output_dir = build_dir.join(&project_info.name);
	let mut out_str = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" ToolsVersion="4.0" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemGroup Label="ProjectConfigurations">
"#
	.to_owned();
	for (profile_name, profile_cfg) in profiles {
		out_str += &format!(
			r#"    <ProjectConfiguration Include="{}|{}">
      <Configuration>{}</Configuration>
      <Platform>{}</Platform>
    </ProjectConfiguration>
"#,
			profile_name, profile_cfg.platform, profile_name, profile_cfg.platform
		);
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
	for (profile_name, profile_cfg) in profiles {
		out_str += &format!(
			r#"    <PropertyGroup Condition="'$(Configuration)|$(Platform)'=='{}|{}'" Label="Configuration">
      <ConfigurationType>{configuration_type}</ConfigurationType>
      <PlatformToolset>{PLATFORM_TOOLSET}</PlatformToolset>
    </PropertyGroup>
"#,
			profile_name, profile_cfg.platform
		);
	}
	out_str += "    <Import Project=\"$(VCTargetsPath)\\Microsoft.Cpp.props\" />\n";
	out_str += "    <PropertyGroup>\n";
	for (profile_name, profile_cfg) in profiles {
		let subfolder = profile_name.clone() + "-" + &profile_cfg.platform;
		let out_dir = output_dir.join(subfolder);
		let out_dir = out_dir.to_string_lossy();
		out_str += &format!(
			r#"    <OutDir Condition="'$(Configuration)|$(Platform)'=='{}|{}'">{out_dir}\\</OutDir>
	    <IntDir Condition="'$(Configuration)|$(Platform)'=='{}|{}'">{target_name}.dir\Debug\</IntDir>
	    <TargetName Condition="'$(Configuration)|$(Platform)'=='{}|{}'">{target_name}</TargetName>
	    <TargetExt Condition="'$(Configuration)|$(Platform)'=='{}|{}'">{target_ext}</TargetExt>
	"#,
			profile_name,
			profile_cfg.platform,
			profile_name,
			profile_cfg.platform,
			profile_name,
			profile_cfg.platform,
			profile_name,
			profile_cfg.platform
		);
	}
	out_str += "  </PropertyGroup>\n";

	// let include_dirs = include_dirs.iter().map(|x| input_path(x, &project_path)).collect::<Vec<String>>();
	// let compile_flags = Vec::new(); // TODO(Travers)
	let compile_as_c = sources.cpp.is_empty() && !sources.c.is_empty();
	for (profile_name, profile) in profiles {
		out_str += &item_definition_group(profile_name, profile, includes, defines, opts, compile_as_c);
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

	fn add_project_references(
		project_links: &Vec<LinkPtr>,
		guid_map: &HashMap<LinkPtr, VsProject>,
		dependencies: &mut Vec<VsProject>,
		build_dir: &Path,
	) -> String {
		let mut out_str = String::new();
		for link in project_links {
			let proj_ref = match guid_map.get(link) {
				Some(x) => x,
				None => {
					out_str += &add_project_references(&link.public_links(), guid_map, dependencies, build_dir);
					continue;
				}
			};
			match link {
				LinkPtr::Static(x) => {
					dependencies.push(proj_ref.clone());
					let proj_ref_include = build_dir.join(&proj_ref.vcxproj_path);
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
						x.name()
					);
				}
				LinkPtr::Interface(_) => {}
			}
		}
		out_str
	}
	let mut dependencies = Vec::new();
	if !project_links.is_empty() {
		out_str += "  <ItemGroup>\n";
		out_str += &add_project_references(project_links, guid_map, &mut dependencies, build_dir);
		out_str += "  </ItemGroup>\n";
	}
	out_str += r#"  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.targets" />
  <ImportGroup Label="ExtensionTargets" />
</Project>"#;
	let vcxproj_pathbuf = PathBuf::from(&project_info.name).join(target_name.to_owned() + ".vcxproj");
	let vcxproj_pathbuf_abs = build_dir.join(&vcxproj_pathbuf);
	let vcxproj_path = vcxproj_pathbuf.to_string_lossy().into_owned();
	let vsproj = VsProject {
		name: target_name.to_owned(),
		guid: target_guid,
		vcxproj_path,
		dependencies,
	};

	// match fs::OpenOptions::new()
	// .create(true)
	// .write(true)
	// .open(vcxproj_pathbuf.clone())
	if let Err(e) = fs::create_dir_all(vcxproj_pathbuf_abs.parent().unwrap()) {
		return Err(format!("Error creating directory for \"{}\": {}", vcxproj_pathbuf.to_string_lossy(), e));
	};
	let mut f = match fs::File::create(vcxproj_pathbuf_abs.clone()) {
		Ok(x) => x,
		Err(e) => return Err(format!("Error creating vcxproj at \"{}\": {}", vcxproj_pathbuf.to_string_lossy(), e)),
	};
	if let Err(e) = f.write_all(out_str.as_bytes()) {
		return Err(format!("Error writing to vcxproj: {}", e));
	}
	Ok(vsproj)
}
