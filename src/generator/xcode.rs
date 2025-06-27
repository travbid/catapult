mod compiler;

use core::{cmp, hash};

use std::{
	collections::BTreeMap, //
	fmt::Write as FmtWrite,
	fs,
	io::Write as IoWrite,
	path,
	path::Path,
	sync::Arc,
};

use crate::{
	executable::Executable,
	misc::{index_map::IndexMap, Sources},
	project::Project,
	toolchain::{compiler::Compiler, PbxItem, Toolchain, XcodeprojectProfile},
	GlobalOptions,
};

#[derive(Clone)]
struct ProjectPtr(Arc<Project>);
impl cmp::PartialEq for ProjectPtr {
	fn eq(&self, other: &ProjectPtr) -> bool {
		Arc::ptr_eq(&self.0, &other.0)
	}
}
impl cmp::Eq for ProjectPtr {}
impl cmp::PartialOrd for ProjectPtr {
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.0.info.name.partial_cmp(&other.0.info.name)
	}
}
impl cmp::Ord for ProjectPtr {
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.info.name.cmp(&other.0.info.name)
	}
}
impl hash::Hash for ProjectPtr {
	fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
		Arc::as_ptr(&self.0).hash(hasher)
	}
}

type ProjectMap = IndexMap<ProjectPtr, XcodeprojGraph>;

pub struct Xcode {}

impl Xcode {
	pub fn generate(
		project: Arc<Project>,
		build_dir: &Path,
		toolchain: Toolchain,
		global_opts: GlobalOptions,
	) -> Result<(), String> {
		generate_xcodeproj(&project, build_dir, toolchain, global_opts)
	}
}

type Key = i32;
type Map<T> = IndexMap<Key, T>;

struct XcodeprojGraph {
	// Roughly in hierarchical order (the *.xcodeproj file lists them in alphabetical order)
	project: PBXProject,
	groups: Map<PBXGroup>,
	native_targets: Map<PBXNativeTarget>,
	build_rules: Map<PBXBuildRule>,
	headers_build_phases: Map<PBXHeadersBuildPhase>,
	sources_build_phases: Map<PBXSourcesBuildPhase>,
	copy_files_build_phases: Map<PBXCopyFilesBuildPhase>,
	frameworks_build_phases: Map<PBXFrameworksBuildPhase>,
	build_files: Map<PBXBuildFile>,
	container_portals: Map<ContainerPortal>,
	file_references: Map<PBXFileReference>,
	reference_proxies: Map<PBXReferenceProxy>,
	container_item_proxies: Map<PBXContainerItemProxy>,
	target_dependencies: Map<PBXTargetDependency>,
	configuration_lists: Map<XCConfigurationList>,
	build_configurations: Map<XCBuildConfiguration>,
}

impl XcodeprojGraph {
	fn into_string(self, project_name: &str) -> String {
		let mut project_str = String::new();
		project_str += r#"// !$*UTF8*$!
{
	archiveVersion = 1;
	classes = {
	};
	objectVersion = 77;
	objects = {

"#;

		// project_str += "/* Begin PBXAggregateTarget section */\n";
		// project_str += "/* End PBXAggregateTarget section */\n\n";

		project_str += "/* Begin PBXBuildFile section */\n";
		for (_, build_file) in &self.build_files {
			project_str += &match &build_file.file_ref {
				Reference::File(file_ref_ix) => {
					let file_ref = self.file_references.get(file_ref_ix).unwrap();
					format!(
						"		{} /* {} in {} */ = {{isa = PBXBuildFile; fileRef = {} /* {} */; }};\n",
						build_file.id,
						build_file.x_name,
						build_file.x_build_phase,
						file_ref.id,
						file_ref.name.as_ref().unwrap_or(&file_ref.path),
					)
				}
				Reference::Proxy(proxy_ix) => {
					let proxy = self.reference_proxies.get(proxy_ix).unwrap();
					format!(
						"		{} /* {} in {} */ = {{isa = PBXBuildFile; fileRef = {} /* {} */; }};\n",
						build_file.id, build_file.x_name, proxy.path, proxy.id, proxy.path
					)
				}
			}
		}
		project_str += "/* End PBXBuildFile section */\n\n";

		if !self.build_rules.is_empty() {
			project_str += "/* Begin PBXBuildRules section */\n\n";

			for (_, build_rule) in &self.build_rules {
				let PBXBuildRule { id, compiler_spec, file_type, .. } = build_rule;
				let script = build_rule.script.replace(r#"""#, r#"\""#);
				let is_editable = build_rule.is_editable as u8;
				project_str += &format!(
					r#"		{id} /* PBXBuildRule */ = {{
			isa = PBXBuildRule;
			compilerSpec = {compiler_spec};
			fileType = {file_type};
			inputFiles = (
"#
				);
				for input in &build_rule.input_files {
					project_str += &format!("\t\t\t\t\"{input}\",\n");
				}
				project_str += &format!(
					r#"			);
			isEditable = {is_editable};
			outputFiles = (
"#
				);
				for output in &build_rule.output_files {
					project_str += &format!("\t\t\t\t\"{output}\",\n");
				}
				project_str += &format!(
					r#"			);
			script = "{script}";
		}};"#
				);
			}
			project_str += "/* End PBXBuildRules section */\n\n";
		}

		if !self.container_item_proxies.is_empty() {
			project_str += "/* Begin PBXContainerItemProxy section */\n";
			for (_, proxy) in &self.container_item_proxies {
				let container_portal = self.container_portals.get(&proxy.container_portal).unwrap();
				let (container_portal_id, container_portal_name): (&str, &str) = match container_portal {
					ContainerPortal::FileReference(fr_ix) => {
						let file_reference = self.file_references.get(fr_ix).unwrap();
						(&file_reference.id, file_reference.name.as_ref().unwrap_or(&file_reference.path))
					}
					ContainerPortal::Project => (&self.project.id, "Project object"),
				};
				let container_proxy = self.container_item_proxies.get(&proxy.remote_global_id_string).unwrap();
				project_str += &format!(
					r#"		{} /* PBXContainerItemProxy */ = {{
			isa = PBXContainerItemProxy;
			containerPortal = {} /* {} */;
			proxyType = {};
			remoteGlobalIDString = {};
			remoteInfo = {};
		}};
"#,
					proxy.id,
					container_portal_id,
					container_portal_name,
					proxy.proxy_type,
					container_proxy.id,
					proxy.remote_info
				);
			}
			project_str += "/* End PBXContainerItemProxy section */\n\n";
		}
		if !self.copy_files_build_phases.is_empty() {
			project_str += "/* Begin PBXCopyFilesBuildPhase section */\n";
			for (_, build_phase) in &self.copy_files_build_phases {
				project_str += &format!(
					r#"		{} /* CopyFiles */ = {{
			isa = PBXCopyFilesBuildPhase;
			buildActionMask = {};
			dstPath = {};
			dstSubfolderSpec = {};
			files = (
"#,
					build_phase.base.id,
					build_phase.base.build_action_mask,
					build_phase.dst_path,
					build_phase.dst_subfolder_spec
				);
				for file_id in &build_phase.base.files {
					let file = self.build_files.get(file_id).unwrap();
					let file_ref_path = match &file.file_ref {
						Reference::File(file_ref) => &self.file_references.get(file_ref).unwrap().path,
						Reference::Proxy(proxy) => &self.reference_proxies.get(proxy).unwrap().path,
					};
					project_str += &format!("\t\t\t\t{} /* {file_ref_path} in CopyFiles */,\n", file.id);
				}
				project_str += &format!(
					r#"			);
			runOnlyForDeploymentPostprocessing = {};
		}};
"#,
					build_phase.base.run_only_for_deployment_postprocessing as u8
				);
			}
			project_str += "/* End PBXCopyFilesBuildPhase section */\n\n";
		}

		project_str += "/* Begin PBXFileReference section */\n";
		for (_, file_ref) in &self.file_references {
			project_str += &print_pbx_file_reference(&file_ref)
		}
		project_str += "/* End PBXFileReference section */\n\n";

		if !self.frameworks_build_phases.is_empty() {
			project_str += "/* Begin PBXFrameworksBuildPhase section */\n";
			for (_, build_phase) in &self.frameworks_build_phases {
				project_str += &format!(
					r#"		{} /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = {};
			files = (
"#,
					build_phase.id, build_phase.build_action_mask
				);
				for file_id in &build_phase.files {
					let file = self.build_files.get(file_id).unwrap();
					let file_ref_path = match &file.file_ref {
						Reference::File(file_ref_id) => {
							let file_ref = self.file_references.get(file_ref_id).unwrap();
							file_ref.name.as_ref().unwrap_or(&file_ref.path)
						}
						Reference::Proxy(proxy_id) => &self.reference_proxies.get(proxy_id).unwrap().path,
					};
					project_str += &format!("\t\t\t\t{} /* {} in Frameworks */,\n", file.id, file_ref_path);
				}
				project_str += &format!(
					r#"			);
			runOnlyForDeploymentPostprocessing = {};
		}};
"#,
					build_phase.run_only_for_deployment_postprocessing as u8
				);
			}
			project_str += "/* End PBXFrameworksBuildPhase section */\n\n";
		}

		project_str += "/* Begin PBXGroup section */\n";
		for (_, group) in &self.groups {
			project_str += &self.print_pbx_group(&group);
		}
		project_str += "/* End PBXGroup section */\n\n";

		if !self.headers_build_phases.is_empty() {
			project_str += "/* Begin PBXHeadersBuildPhase section */\n";
			for (_, build_phase) in &self.headers_build_phases {
				project_str += &format!(
					r#"		{} /* Headers */ = {{
			isa = PBXHeadersBuildPhase;
			buildActionMask = {};
			files = (
"#,
					build_phase.id, build_phase.build_action_mask
				);
				for file_id in &build_phase.files {
					let file = self.build_files.get(file_id).unwrap();
					let file_ref = match &file.file_ref {
						Reference::File(file_ref) => self.file_references.get(file_ref).unwrap(),
						Reference::Proxy(_) => panic!("TODO"),
					};
					project_str += &format!("\t\t\t\t{} /* {} in Headers */,\n", file.id, file_ref.path);
				}
				project_str += &format!(
					r#"			);
			runOnlyForDeploymentPostprocessing = {};
		}};
"#,
					build_phase.run_only_for_deployment_postprocessing as u8
				);
			}
			project_str += "/* End PBXHeadersBuildPhase section */\n\n";
		}

		project_str += "/* Begin PBXNativeTarget section */\n";
		for (_, native_target) in &self.native_targets {
			project_str += &self.print_pbx_native_target(native_target);
		}
		project_str += "/* End PBXNativeTarget section */\n\n";

		project_str += "/* Begin PBXProject section */\n";
		let pbx_project = &self.project;
		project_str += &format!(
			r#"		{} /* Project object */ = {{
			isa = PBXProject;
			attributes = {{
				BuildIndependentTargetsInParallel = {};
				LastUpgradeCheck = {};
				TargetAttributes = {{
"#,
			pbx_project.id,
			pbx_project.attribute_build_indpendent_targets_in_parallel as u8,
			pbx_project.attribute_last_upgrade_check
		);
		for target_attribute in &pbx_project.attribute_target_attributes {
			project_str += &format!(
				r#"					{} = {{
						{};
					}};
"#,
				target_attribute.0, target_attribute.1
			);
		}
		project_str += &format!(
			r#"				}};
			}};
			buildConfigurationList = {} /* Build configuration list for PBXProject "{project_name}" */;
			developmentRegion = {};
			hasScannedForEncodings = {};
			knownRegions = (
"#,
			self.configuration_lists
				.get(&pbx_project.build_configuration_list)
				.unwrap()
				.id,
			// pbx_project.compatibility_version,
			pbx_project.development_region,
			pbx_project.has_scanned_for_encodings as u8,
		);
		for region in &pbx_project.known_regions {
			project_str += &format!("				{region},\n");
		}
		let product_ref_group = self.groups.get(&pbx_project.product_ref_group).unwrap();
		project_str += &format!(
			r#"			);
			mainGroup = {};
			minimizedProjectReferenceProxies = 1;
			preferredProjectObjectVersion = 77;
			productRefGroup = {} /* {} */;
			projectDirPath = {};
"#,
			self.groups.get(&pbx_project.main_group).unwrap().id,
			product_ref_group.id,
			product_ref_group.name.as_ref().unwrap(),
			pbx_project.project_dir_path,
		);
		if !pbx_project.project_references.is_empty() {
			project_str += "			projectReferences = (\n";
			for (product_group, project_ref) in &pbx_project.project_references {
				let project_ref = self.file_references.get(project_ref).unwrap();
				project_str += &format!(
					r#"				{{
					ProductGroup = {} /* Products */;
					ProjectRef = {} /* {} */;
				}},
"#,
					self.groups.get(&product_group).unwrap().id,
					project_ref.id,
					project_ref.name.as_ref().unwrap()
				);
			}
			project_str += "			);\n";
		}
		project_str += &format!(
			r#"			projectRoot = "{}";
			targets = (
"#,
			pbx_project.project_root,
		);
		for target_id in &pbx_project.targets {
			let target = self.native_targets.get(target_id).unwrap();
			project_str += &format!("				{} /* {} */,\n", target.id, target.name);
		}
		project_str += "			);\n		};\n";
		project_str += "/* End PBXProject section */\n\n";

		if !self.reference_proxies.is_empty() {
			project_str += "/* Begin PBXReferenceProxy section */\n";
			for (_, ref_proxy) in &self.reference_proxies {
				project_str += &format!(
					r#"		{} /* {} */ = {{
			isa = PBXReferenceProxy;
			fileType = {};
"#,
					ref_proxy.id, ref_proxy.path, ref_proxy.file_type,
				);
				if let Some(name) = &ref_proxy.name {
					project_str += &format!("\t\t\tname = {name};\n");
				}
				project_str += &format!(
					r#"			path = {};
			remoteRef = {} /* PBXContainerItemProxy */;
			sourceTree = {};
		}};
"#,
					ref_proxy.path,
					self.container_item_proxies.get(&ref_proxy.remote_ref).unwrap().id,
					ref_proxy.source_tree
				);
			}
			project_str += "/* End PBXReferenceProxy section */\n\n";
		}

		project_str += "/* Begin PBXSourcesBuildPhase section */\n";
		for (_, build_phase) in &self.sources_build_phases {
			project_str += &format!(
				r#"		{} /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = {};
			files = (
"#,
				build_phase.id, build_phase.build_action_mask
			);
			for file_id in &build_phase.files {
				let file = self.build_files.get(file_id).unwrap();
				let file_ref = match &file.file_ref {
					Reference::File(file_ref) => self.file_references.get(file_ref).unwrap(),
					Reference::Proxy(_) => panic!("Unexpected PBXReferenceProxy"),
				};
				project_str += &format!(
					"\t\t\t\t{} /* {} in Sources */,\n",
					file.id,
					file_ref.name.as_ref().unwrap_or(&file_ref.path)
				);
			}
			project_str += &format!(
				r#"			);
			runOnlyForDeploymentPostprocessing = {};
		}};
"#,
				build_phase.run_only_for_deployment_postprocessing as u8
			);
		}
		project_str += "/* End PBXSourcesBuildPhase section */\n\n";

		if !self.target_dependencies.is_empty() {
			project_str += "/* Begin PBXTargetDependency section */\n";
			for (_, target_dependency) in &self.target_dependencies {
				let target = self.native_targets.get(&target_dependency.target).unwrap();
				let target_proxy = self
					.container_item_proxies
					.get(&target_dependency.target_proxy)
					.unwrap();
				project_str += &format!(
					r#"		{} /* PBXTargetDependency */ = {{
			isa = PBXTargetDependency;
			target = {} /* {} */;
			targetProxy = {} /* PBXContainerItemProxy */;
		}};
"#,
					target_dependency.id, target.id, target.name, target_proxy.id
				);
			}
			project_str += "/* End PBXTargetDependency section */\n\n";
		}

		project_str += "/* Begin XCBuildConfiguration section */\n";
		for (_, build_configuration) in &self.build_configurations {
			project_str += &print_xc_build_configuration(build_configuration);
		}
		project_str += "/* End XCBuildConfiguration section */\n\n";

		project_str += "/* Begin XCConfigurationList section */\n";
		for (_, configuration_list) in &self.configuration_lists {
			project_str += &self.print_xc_configuration_list(configuration_list);
		}
		project_str += "/* End XCConfigurationList section */\n";

		project_str += &format!(
			r#"	}};
	rootObject = {} /* Project object */;
}}
"#,
			pbx_project.id
		);

		project_str
	}

	fn print_pbx_group(&self, group: &PBXGroup) -> String {
		let mut ret = String::new();
		let comment = if let Some(name) = &group.name {
			"/* ".to_owned() + name + " */ "
		} else if let Some(path) = &group.path {
			"/* ".to_owned() + path + " */ "
		} else {
			String::new()
		};
		ret += &format!(
			r#"		{} {}= {{
			isa = PBXGroup;
			children = (
"#,
			group.id, comment
		);
		for child_id in &group.children {
			if let Some(group) = self.groups.get(child_id) {
				if let Some(name) = group.name.as_ref() {
					ret += &format!("\t\t\t\t{} /* {name} */,\n", group.id);
				} else if let Some(path) = group.path.as_ref() {
					ret += &format!("\t\t\t\t{} /* {path} */,\n", group.id);
				} else {
					ret += &format!("\t\t\t\t{},\n", group.id);
				}
			} else if let Some(file_ref) = self.file_references.get(child_id) {
				let path = file_ref.name.as_ref().unwrap_or(&file_ref.path);
				ret += &format!("\t\t\t\t{} /* {} */,\n", file_ref.id, path);
			} else if let Some(proxy) = self.reference_proxies.get(child_id) {
				ret += &format!("\t\t\t\t{} /* {} */,\n", proxy.id, proxy.path);
			} else {
				panic!("Could not find {child_id}");
			}
		}
		ret += "			);\n";
		if let Some(name) = &group.name {
			ret += &format!("			name = {};\n", name);
		}
		if let Some(path) = &group.path {
			ret += &format!("			path = {};\n", path);
		}
		ret += &format!(
			r#"			sourceTree = {};
		}};
"#,
			group.source_tree
		);
		ret
	}

	fn print_pbx_native_target(&self, native_target: &PBXNativeTarget) -> String {
		let build_configuration_list = self
			.configuration_lists
			.get(&native_target.build_configuration_list)
			.unwrap();
		let mut ret = format!(
			r#"		{} /* {} */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = {} /* Build configuration list for PBXNativeTarget "{}" */;
			buildPhases = (
"#,
			native_target.id, native_target.name, build_configuration_list.id, native_target.name
		);
		for build_phase_id in &native_target.build_phases {
			if let Some(build_phase) = self.headers_build_phases.get(build_phase_id) {
				ret += &format!("				{} /* Headers */,\n", build_phase.id);
			} else if let Some(build_phase) = self.sources_build_phases.get(build_phase_id) {
				ret += &format!("				{} /* Sources */,\n", build_phase.id);
			} else if let Some(build_phase) = self.frameworks_build_phases.get(build_phase_id) {
				ret += &format!("				{} /* Frameworks */,\n", build_phase.id);
			} else if let Some(build_phase) = self.copy_files_build_phases.get(build_phase_id) {
				ret += &format!("				{} /* CopyFiles */,\n", build_phase.base.id);
			} else {
				panic!("TODO");
			}
		}
		ret += r#"			);
			buildRules = (
"#;
		for build_rule_key in &native_target.build_rules {
			let build_rule = self.build_rules.get(build_rule_key).unwrap();
			ret += &format!("				{},\n", build_rule.id);
		}
		ret += r#"			);
			dependencies = (
"#;
		for dependency in &native_target.dependencies {
			let target_dependency = self.target_dependencies.get(dependency).unwrap();
			ret += &format!("				{} /* PBXTargetDependency */,\n", target_dependency.id);
		}
		let product_reference = self.file_references.get(&native_target.product_reference).unwrap();
		ret += &format!(
			r#"			);
			name = {};
			packageProductDependencies = (
			);
			productName = {};
			productReference = {} /* {} */;
			productType = "{}";
		}};
"#,
			native_target.name,
			native_target.product_name,
			product_reference.id,
			product_reference.name.as_ref().unwrap_or(&product_reference.path),
			native_target.product_type
		);
		ret
	}

	fn print_xc_configuration_list(&self, build_configuration_list: &XCConfigurationList) -> String {
		let mut ret = format!(
			r#"		{} /* Build configuration list for {} "{}" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
"#,
			build_configuration_list.id, build_configuration_list.x_target_type, build_configuration_list.x_target_name
		);
		for build_config_id in &build_configuration_list.build_configurations {
			let build_config = self.build_configurations.get(build_config_id).unwrap();
			ret += &format!("				{} /* {} */,\n", build_config.id, build_config.name);
		}
		ret += &format!(
			r#"			);
			defaultConfigurationIsVisible = {};
			defaultConfigurationName = {};
		}};
"#,
			build_configuration_list.default_configuration_is_visible as u8,
			build_configuration_list.default_configuration_name
		);
		ret
	}
} // impl XcodeprojGraph

fn print_pbx_file_reference(file_ref: &PBXFileReference) -> String {
	let mut ret = format!(
		"		{} /* {} */ = {{isa = PBXFileReference; ",
		file_ref.id,
		file_ref.name.as_ref().unwrap_or(&file_ref.path)
	);
	match &file_ref.file_type {
		FileRefType::Explicit(file_type) => ret += &format!("explicitFileType = {file_type}; "),
		FileRefType::LastKnown(file_type) => ret += &format!("lastKnownFileType = {file_type}; "),
	};
	if let Some(include) = file_ref.include_in_index {
		ret += &format!("includeInIndex = {}; ", include as u8);
	}
	if let Some(name) = &file_ref.name {
		ret += &format!("name = {name}; ");
	}
	ret += &format!("path = {}; sourceTree = {}; }};\n", file_ref.path, file_ref.source_tree);
	ret
}

fn print_xc_build_configuration(build_config: &XCBuildConfiguration) -> String {
	let mut ret = String::new();
	ret += &format!(
		r#"		{} /* {} */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
"#,
		build_config.id, build_config.name,
	);
	for (key, value) in &build_config.build_settings {
		ret += &format!("				{} = {:\t>4};\n", key, value);
	}
	ret += &format!(
		r#"			}};
			name = {};
		}};
"#,
		build_config.name
	);
	ret
}

fn generate_xcodeproj(
	project: &Arc<Project>,
	build_dir: &Path,
	toolchain: Toolchain,
	global_opts: GlobalOptions,
) -> Result<(), String> {
	let pbx_projects = transform_build_graph_to_xcode_graphs(project.clone(), toolchain, &global_opts, Path::new(""))?;
	for (subproject, xcodeproj) in pbx_projects {
		let xcodeproj_str = xcodeproj.into_string(&subproject.0.info.name);
		let pbxproj_path = build_dir
			.join(&subproject.0.info.name)
			.join(subproject.0.info.name.clone() + ".xcodeproj")
			.join("project.pbxproj");
		if let Err(e) = fs::create_dir_all(pbxproj_path.parent().unwrap()) {
			return Err(format!("Error creating directory for \"{}\": {}", pbxproj_path.to_string_lossy(), e));
		};

		let mut f = match fs::File::create(&pbxproj_path) {
			Ok(x) => x,
			Err(e) => return Err(format!("Error creating file at \"{}\": {}", pbxproj_path.to_string_lossy(), e)),
		};

		if let Err(e) = f.write_all(xcodeproj_str.as_bytes()) {
			return Err(format!("Error writing to {}: {}", pbxproj_path.to_string_lossy(), e));
		}
	}
	Ok(())
}

struct IdGenerator {
	str_counter: i32,
	int_counter: i32,
}

impl IdGenerator {
	fn next(&mut self) -> i32 {
		self.int_counter += 1;
		self.int_counter
	}
	fn new_id(&mut self, msg: &str) -> String {
		let ret = format!("{:024X}", self.str_counter);
		self.str_counter += 1;
		ret
	}
}

struct SubGraph {
	// project: PBXProject,
	groups: Map<PBXGroup>,
	native_targets: Map<PBXNativeTarget>,
	build_rules: Map<PBXBuildRule>,
	headers_build_phases: Map<PBXHeadersBuildPhase>,
	sources_build_phases: Map<PBXSourcesBuildPhase>,
	copy_files_build_phases: Map<PBXCopyFilesBuildPhase>,
	frameworks_build_phases: Map<PBXFrameworksBuildPhase>,
	build_files: Map<PBXBuildFile>,
	container_portals: Map<ContainerPortal>,
	file_references: Map<PBXFileReference>,
	reference_proxies: Map<PBXReferenceProxy>,
	container_item_proxies: Map<PBXContainerItemProxy>,
	target_dependencies: Map<PBXTargetDependency>,
	configuration_lists: Map<XCConfigurationList>,
	build_configurations: Map<XCBuildConfiguration>,
}

fn transform_build_graph_to_xcode_graphs(
	project: Arc<Project>,
	toolchain: Toolchain,
	global_opts: &GlobalOptions,
	build_dir: &Path,
) -> Result<ProjectMap, String> {
	let mut profiles = toolchain
		.profile
		.iter()
		.filter_map(|x| x.1.xcodeproj.as_ref().map(|prof| (x.0.clone(), prof.clone())))
		.collect::<BTreeMap<String, XcodeprojectProfile>>();
	if profiles.is_empty() {
		return Err(
			"Toolchain doesn't contain any profiles with a \"xcodeproj\" section, required for Xcode generator"
				.to_owned(),
		);
	}
	let compiler = compiler::Xcode {};
	if let Some(c_std) = &global_opts.c_standard {
		for profile in profiles.values_mut() {
			profile
				.project
				.insert("GCC_C_LANGUAGE_STANDARD".to_owned(), PbxItem::String(compiler.c_std_flag(c_std)?));
		}
	}
	if let Some(cpp_std) = &global_opts.cpp_standard {
		for profile in profiles.values_mut() {
			profile
				.project
				.insert("CLANG_CXX_LANGUAGE_STANDARD".to_owned(), PbxItem::String(compiler.cpp_std_flag(cpp_std)?));
		}
	}
	let mut projects = ProjectMap::new();
	let mut id_gen = IdGenerator { str_counter: 0, int_counter: 0 };
	transform_graph_inner(project, &profiles, global_opts, build_dir, &toolchain, &mut projects, &mut id_gen)?;
	Ok(projects)
}

fn transform_graph_inner(
	project: Arc<Project>,
	profiles: &BTreeMap<String, XcodeprojectProfile>,
	global_opts: &GlobalOptions,
	build_dir: &Path,
	toolchain: &Toolchain,
	projects: &mut ProjectMap,
	id_gen: &mut IdGenerator,
) -> Result<(), String> {
	let mut graph = SubGraph {
		groups: Map::new(),
		native_targets: Map::new(),
		build_rules: Map::new(),
		headers_build_phases: Map::new(),
		sources_build_phases: Map::new(),
		copy_files_build_phases: Map::new(),
		frameworks_build_phases: Map::new(),
		build_files: Map::new(),
		container_portals: Map::new(),
		file_references: Map::new(),
		reference_proxies: Map::new(),
		container_item_proxies: Map::new(),
		target_dependencies: Map::new(),
		configuration_lists: Map::new(),
		build_configurations: Map::new(),
	};
	let project_build_configuration_list_id = id_gen.next();
	{
		let mut project_xc_build_configurations = Vec::new();
		for (profile_name, profile) in profiles {
			let key = id_gen.next();
			graph.build_configurations.insert(
				key,
				XCBuildConfiguration {
					id: id_gen.new_id("project xc build config"),
					name: profile_name.clone(),
					build_settings: profile
						.project
						.iter()
						.map(|(key, value)| {
							(
								key.clone(),
								match value {
									PbxItem::String(item) => BuildSetting::Single(item.clone()),
									PbxItem::Vec(item) => BuildSetting::Array(item.clone()),
								},
							)
						})
						.collect(),
				},
			);
			project_xc_build_configurations.push(key);
		}
		let project_build_configuration_list = XCConfigurationList {
			id: id_gen.new_id("project xc build config"),
			build_configurations: project_xc_build_configurations,
			default_configuration_is_visible: false,
			default_configuration_name: profiles.first_key_value().unwrap().0.clone(), // TODO(Travers)
			x_target_name: project.info.name.clone(),
			x_target_type: "PBXProject".to_owned(),
		};
		graph
			.configuration_lists
			.insert(project_build_configuration_list_id, project_build_configuration_list);
	}

	let native_target_build_configurations: Vec<XCBuildConfiguration> = profiles
		.iter()
		.map(|(profile_name, profile)| XCBuildConfiguration {
			id: "-".to_owned(),
			name: profile_name.clone(),
			build_settings: profile
				.native_target
				.iter()
				.map(|(key, value)| {
					(
						key.clone(),
						match value {
							PbxItem::String(item) => BuildSetting::Single(item.clone()),
							PbxItem::Vec(item) => BuildSetting::Array(item.clone()),
						},
					)
				})
				.collect(),
		})
		.collect();

	let native_targets = project_targets(&project, &native_target_build_configurations, toolchain, &mut graph, id_gen)?;

	let product_group_key = id_gen.next();
	{
		let mut children = Vec::new();
		for target in &native_targets {
			let nt = graph.native_targets.get(target).unwrap();
			children.push(nt.product_reference);
		}
		let product_group = PBXGroup {
			id: id_gen.new_id("group"),
			children,
			name: Some("Products".to_owned()),
			path: None,
			source_tree: "\"<group>\"".to_owned(),
		};
		graph.groups.insert(product_group_key, product_group);
	}
	let mut main_group_children = Vec::new();
	for nt_id in &native_targets {
		let target = graph.native_targets.get(nt_id).unwrap();
		let mut children = Vec::new();
		for build_phase_key in &target.build_phases {
			let file_keys = if let Some(build_phase) = graph.headers_build_phases.get(build_phase_key) {
				build_phase.files.clone()
			} else if let Some(build_phase) = graph.sources_build_phases.get(build_phase_key) {
				build_phase.files.clone()
			} else if let Some(build_phase) = graph.frameworks_build_phases.get(build_phase_key) {
				build_phase.files.clone()
			} else {
				panic!("TODO");
			};
			if file_keys.is_empty() {
				continue;
			}
			children.extend(
				file_keys
					.iter()
					.map(|key| match graph.build_files.get(key).unwrap().file_ref {
						Reference::File(x) => x,
						Reference::Proxy(x) => x,
					}),
			);
		}
		let child_id = id_gen.next();
		graph.groups.insert(
			child_id,
			PBXGroup {
				id: id_gen.new_id("main group child"),
				children,
				name: Some(target.name.clone()),
				path: None,
				source_tree: "\"<group>\"".to_owned(),
			},
		);
		main_group_children.push(child_id);
	}
	main_group_children.push(product_group_key);
	// main_group_children.push(frameworks_group_key);

	let main_group_key = id_gen.next();
	graph.groups.insert(
		main_group_key,
		PBXGroup {
			id: id_gen.new_id("main group"),
			children: main_group_children,
			name: None,
			path: None,
			source_tree: "\"<group>\"".to_owned(),
		},
	);

	let pbx_project = PBXProject {
		id: id_gen.new_id("project"),
		attribute_build_indpendent_targets_in_parallel: true,
		attribute_last_upgrade_check: 2610,
		attribute_target_attributes: native_targets
			.iter()
			.map(|x| (graph.native_targets.get(x).unwrap().id.clone(), "CreatedOnToolsVersion = 26.1.1".to_owned()))
			.collect(),
		build_configuration_list: project_build_configuration_list_id,
		compatibility_version: "\"Xcode 14.0\"".to_owned(),
		development_region: "en".to_owned(),
		has_scanned_for_encodings: false,
		known_regions: vec!["en".to_owned(), "Base".to_owned()],
		main_group: main_group_key,
		product_ref_group: product_group_key,
		project_dir_path: path::absolute(&project.info.path)
			.unwrap()
			.to_string_lossy()
			.to_string(),
		project_references: Vec::new(),
		project_root: String::new(),
		targets: graph.native_targets.keys().collect(),
	};

	projects.insert(
		ProjectPtr(project.clone()),
		XcodeprojGraph {
			project: pbx_project,
			groups: graph.groups,
			native_targets: graph.native_targets,
			build_rules: graph.build_rules,
			headers_build_phases: graph.headers_build_phases,
			sources_build_phases: graph.sources_build_phases,
			copy_files_build_phases: graph.copy_files_build_phases,
			frameworks_build_phases: graph.frameworks_build_phases,
			build_files: graph.build_files,
			container_portals: graph.container_portals,
			file_references: graph.file_references,
			reference_proxies: graph.reference_proxies,
			container_item_proxies: graph.container_item_proxies,
			target_dependencies: graph.target_dependencies,
			configuration_lists: graph.configuration_lists,
			build_configurations: graph.build_configurations,
		},
	);
	Ok(())
}

fn project_targets(
	project: &Project,
	native_target_build_configs: &[XCBuildConfiguration],
	toolchain: &Toolchain,
	graph: &mut SubGraph,
	id_gen: &mut IdGenerator,
) -> Result<Vec<Key>, String> {
	let mut ret = Vec::new();
	for exe in &project.executables {
		ret.push(new_native_target_executable(exe, native_target_build_configs, toolchain, graph, id_gen)?);
	}
	Ok(ret)
}

fn new_native_target_executable(
	exe: &Arc<Executable>,
	native_target_build_configs: &[XCBuildConfiguration],
	toolchain: &Toolchain,
	graph: &mut SubGraph,
	id_gen: &mut IdGenerator,
) -> Result<Key, String> {
	let product_reference = id_gen.next();
	graph.file_references.insert(
		product_reference,
		PBXFileReference {
			id: id_gen.new_id("exe fileref"),
			file_type: FileRefType::Explicit(ExplicitFileType::Executable),
			include_in_index: Some(false),
			name: None,
			path: exe.name.clone(),
			source_tree: "BUILT_PRODUCTS_DIR".to_owned(),
		},
	);

	if let Some(_) = exe.generator_vars.as_ref() {
		return Err("generator_vars are not supported with Xcode generator".to_owned());
		// let gen_sources = self.evaluate_generator_vars(generator_vars, &lib.project().info.path, toolchain)?;
		// &lib.sources.extended_with(gen_sources)
	}
	let include_dirs = exe
		.public_includes_recursive()
		.into_iter()
		.map(|src| src.to_string_lossy().to_string())
		.collect::<Vec<String>>();

	let build_phases = add_build_phases(&exe.sources, graph, id_gen);
	let mut build_rule_keys = Vec::new();
	if !exe.sources.nasm.is_empty() {
		for platform in &toolchain.xcode_platforms {
			if platform != "x86_64" {
				return Err(format!("NASM sources can not be built for xcode platform {platform}"));
			}
		}
		let nasm_cmd = match &toolchain.nasm_assembler {
			None => {
				return Err(
					"Toolchain does not contain a NASM assembler, required for files in this project".to_owned()
				);
			}
			Some(x) => x,
		}
		.cmd()
		.join(" ");
		let build_rule_key = id_gen.next();
		graph.build_rules.insert(
			build_rule_key,
			PBXBuildRule {
				id: id_gen.new_id("build rule nasm"),
				compiler_spec: CompilerSpec::ProxyScript,
				file_type: FileType::Nasm,
				// inputFiles = ("$(SRCROOT)/submodules/nasmproj/nasmsrc.asm",);
				// outputFiles = ("$(DERIVED_FILE_DIR)/$(INPUT_FILE_BASE).o",);
				input_files: exe
					.sources
					.nasm
					.iter()
					.map(|src| src.full.to_string_lossy().to_string())
					.collect(),
				is_editable: true,
				output_files: exe
					.sources
					.nasm
					.iter()
					.map(|src| format!("$(DERIVED_FILE_DIR)/{}.o", src.name))
					.collect(),
				script: format!("set -x\n{nasm_cmd} -o \"$SCRIPT_OUTPUT_FILE_0\" \"$SCRIPT_INPUT_FILE\"\n"),
			},
		);
		build_rule_keys.push(build_rule_key);
	}
	let native_target_key = id_gen.next();
	let native_target = PBXNativeTarget {
		id: id_gen.new_id(&("native_target: ".to_owned() + &exe.name)),
		build_configuration_list: clone_xc_with(
			native_target_build_configs,
			graph,
			include_dirs,
			exe.name.clone(),
			id_gen,
		),
		build_phases,
		build_rules: build_rule_keys,
		dependencies: Vec::new(),
		name: exe.name.clone(),
		product_name: exe.name.clone(),
		product_reference,
		product_type: ProductType::Tool,
	};
	graph.native_targets.insert(native_target_key, native_target);
	Ok(native_target_key)
}

fn add_build_phases(sources: &Sources, graph: &mut SubGraph, id_gen: &mut IdGenerator) -> Vec<Key> {
	let mut build_phase_keys = Vec::new();
	if !sources.h.is_empty() {
		let mut files = Vec::new();
		for src_path in &sources.h {
			let file_ref_key = id_gen.next();
			graph.file_references.insert(
				file_ref_key,
				PBXFileReference {
					id: id_gen.new_id(&src_path.name),
					file_type: FileRefType::LastKnown(FileType::Header),
					include_in_index: None,
					name: None,                  //Some(src_path.name.clone()),
					path: src_path.name.clone(), //.full.to_string_lossy().to_string(), // TODO: relative?
					source_tree: "SOURCE_ROOT".to_owned(),
				},
			);
			let build_file_key = id_gen.next();
			graph.build_files.insert(
				build_file_key,
				PBXBuildFile {
					id: id_gen.new_id("build_file"),
					file_ref: Reference::File(file_ref_key),
					x_name: src_path.name.clone(),
					x_build_phase: "Headers".to_owned(),
				},
			);
			files.push(build_file_key);
		}
		let build_phase_key = id_gen.next();
		graph.headers_build_phases.insert(
			build_phase_key,
			PBXHeadersBuildPhase {
				id: id_gen.new_id("headers"),
				build_action_mask: 0x7F_FF_FF_FF, // 2147483647
				files,
				run_only_for_deployment_postprocessing: false,
			},
		);
		build_phase_keys.push(build_phase_key);
	}
	{
		let mut files = Vec::new();
		for src_path in &sources.c {
			let file_ref_key = id_gen.next();
			graph.file_references.insert(
				file_ref_key,
				PBXFileReference {
					id: id_gen.new_id(&src_path.name),
					file_type: FileRefType::LastKnown(FileType::C),
					include_in_index: None,
					name: None,                  //Some(src_path.name.clone()),
					path: src_path.name.clone(), //.full.to_string_lossy().to_string(), // TODO: relative?
					source_tree: "SOURCE_ROOT".to_owned(),
				},
			);
			let build_file_key = id_gen.next();
			graph.build_files.insert(
				build_file_key,
				PBXBuildFile {
					id: id_gen.new_id("build_file"),
					file_ref: Reference::File(file_ref_key),
					x_name: src_path.name.clone(),
					x_build_phase: "Sources".to_owned(),
				},
			);
			files.push(build_file_key);
		}
		for src_path in &sources.cpp {
			let file_ref_key = id_gen.next();
			graph.file_references.insert(
				file_ref_key,
				PBXFileReference {
					id: id_gen.new_id(&src_path.name),
					file_type: FileRefType::LastKnown(FileType::Cpp),
					include_in_index: None,
					name: None,                  //Some(src_path.name.clone()),
					path: src_path.name.clone(), //.full.to_string_lossy().to_string(), // TODO: relative?
					source_tree: "SOURCE_ROOT".to_owned(),
				},
			);
			let build_file_key = id_gen.next();
			graph.build_files.insert(
				build_file_key,
				PBXBuildFile {
					id: id_gen.new_id("build_file"),
					file_ref: Reference::File(file_ref_key),
					x_name: src_path.name.clone(),
					x_build_phase: "Sources".to_owned(),
				},
			);
			files.push(build_file_key);
		}
		if !files.is_empty() {
			let build_phase_key = id_gen.next();
			graph.sources_build_phases.insert(
				build_phase_key,
				BuildPhaseBase {
					id: id_gen.new_id("sources"),
					build_action_mask: 0x7F_FF_FF_FF, // 2147483647
					files,
					run_only_for_deployment_postprocessing: false,
				},
			);
			build_phase_keys.push(build_phase_key);
		}
	}
	// CopyFiles
	build_phase_keys
}

fn clone_xc_with(
	native_target_build_configs: &[XCBuildConfiguration],
	graph: &mut SubGraph,
	include_dirs: Vec<String>,
	target_name: String,
	id_gen: &mut IdGenerator,
) -> Key {
	let mut build_configuration_keys = Vec::new();
	for build_cfg in native_target_build_configs {
		let mut build_settings = build_cfg.build_settings.clone();
		if let Some(sett) = build_settings.iter_mut().find(|(key, _)| *key == "HEADER_SEARCH_PATHS") {
			match sett.1 {
				BuildSetting::Array(ref mut arr) => arr.extend(include_dirs.clone()),
				BuildSetting::Single(item) => {
					let mut inc_dirs = vec![item.clone()];
					inc_dirs.extend(include_dirs.clone());
					build_settings.insert("HEADER_SEARCH_PATHS".to_owned(), BuildSetting::Array(inc_dirs));
				}
			}
		} else if !include_dirs.is_empty() {
			build_settings.insert("HEADER_SEARCH_PATHS".to_owned(), BuildSetting::Array(include_dirs.clone()));
		}
		let build_cfg_key = id_gen.next();
		graph.build_configurations.insert(
			build_cfg_key,
			XCBuildConfiguration {
				id: id_gen.new_id("xc build clone"),
				build_settings,
				name: build_cfg.name.clone(),
			},
		);
		build_configuration_keys.push(build_cfg_key);
	}

	let cfg_list_key = id_gen.next();
	let default_configuration_name = graph
		.build_configurations
		.get(build_configuration_keys.first().unwrap())
		.unwrap()
		.name
		.clone();
	graph.configuration_lists.insert(
		cfg_list_key,
		XCConfigurationList {
			id: id_gen.new_id("xc config clone"),
			build_configurations: build_configuration_keys,
			default_configuration_is_visible: false,
			default_configuration_name,
			x_target_name: target_name,
			x_target_type: "PBXNativeTarget".to_owned(),
		},
	);
	cfg_list_key
}

struct PBXProject {
	pub id: String,
	// isa = PBXProject;
	// attributes = {
	// 	BuildIndependentTargetsInParallel = 1;
	pub attribute_build_indpendent_targets_in_parallel: bool,
	// 	LastUpgradeCheck = 1640;
	pub attribute_last_upgrade_check: u32, // = 1640;
	// 	TargetAttributes = {
	// 		5D654BE92C4CCF39003465E3 = {
	// 			CreatedOnToolsVersion = 15.4;
	// 		};
	// 		5DEA4D542C43D053008D0969 = {
	// 			CreatedOnToolsVersion = 15.4;
	// 		};
	// 	};
	pub attribute_target_attributes: Vec<(String, String)>, // PBXNativeTarget, String
	// };
	// buildConfigurationList = 5DEA4D502C43D053008D0969 /* Build configuration list for PBXProject "example" */;
	pub build_configuration_list: Key, // XCConfigurationList
	// compatibilityVersion = "Xcode 14.0";
	pub compatibility_version: String,
	// developmentRegion = en;
	pub development_region: String,
	// hasScannedForEncodings = 0;
	pub has_scanned_for_encodings: bool,
	// knownRegions = (en,Base);
	pub known_regions: Vec<String>,
	// mainGroup = 5DEA4D4C2C43D053008D0969;
	pub main_group: Key, // PBXGroup
	// productRefGroup = 5DEA4D562C43D053008D0969 /* Products */;
	pub product_ref_group: Key, // PBXGroup
	// projectDirPath = "";
	pub project_dir_path: String,
	pub project_references: Vec<(
		Key, // PBXGroup // ProductGroup
		Key, // PBXFileReference // ProjectRef
	)>,
	// projectRoot = "";
	pub project_root: String,
	// targets = (
	// 	5DEA4D542C43D053008D0969 /* example */,
	// 	5D654BE92C4CCF39003465E3 /* exmath */,
	// );
	pub targets: Vec<Key>, // PBXNativeTarget[]
}

struct PBXNativeTarget {
	pub id: String,
	// isa = PBXNativeTarget;
	// buildConfigurationList = 5D654BED2C4CCF39003465E3 /* Build configuration list for PBXNativeTarget "exmath" */;
	pub build_configuration_list: Key, // XCConfigurationList
	// buildPhases = (
	// 	5D654BE62C4CCF39003465E3 /* Headers */,
	// 	5D654BE72C4CCF39003465E3 /* Sources */,
	// 	5D654BE82C4CCF39003465E3 /* Frameworks */,
	// );
	pub build_phases: Vec<Key>, // BuildPhase[]
	// buildRules = ();
	pub build_rules: Vec<Key>, // PBXBuildRule[]
	// dependencies = ();
	pub dependencies: Vec<Key>, // PBXTargetDependency[]
	// name = exmath;
	pub name: String,
	// productName = exmath;
	pub product_name: String,
	// productReference = 5D654BEA2C4CCF39003465E3 /* libexmath.a */;
	pub product_reference: Key, // PBXFileReference
	// productType = "com.apple.product-type.library.static";
	pub product_type: ProductType,
}

struct PBXBuildRule {
	pub id: String,
	// isa = PBXBuildRule;
	// compilerSpec = com.apple.compilers.proxy.script;
	pub compiler_spec: CompilerSpec,
	// fileType = sourcecode.nasm;
	pub file_type: FileType,
	// inputFiles = ("$(SRCROOT)/submodules/nasmproj/nasmsrc.asm",);
	pub input_files: Vec<String>,
	// isEditable = 1;
	is_editable: bool,
	// outputFiles = ("$(DERIVED_FILE_DIR)/$(INPUT_FILE_BASE).o",);
	pub output_files: Vec<String>,
	// script = "set -x\nprintenv\nprintenv | grep 86\nprintenv | grep 64\nls -l /Users/travers/Downloads/nasm-2.16.03/nasm\nwhoami\n/Users/travers/Downloads/nasm-2.16.03/nasm \"-fmacho64\" \"-o\" $SCRIPT_OUTPUT_FILE_0 $SCRIPT_INPUT_FILE\npwd\n";
	pub script: String,
}

enum CompilerSpec {
	ProxyScript,
}

impl core::fmt::Display for CompilerSpec {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			CompilerSpec::ProxyScript => f.write_str("com.apple.compilers.proxy.script"),
		}
	}
}

enum ProductType {
	LibraryStatic,
	Tool,
}

impl core::fmt::Display for ProductType {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			ProductType::LibraryStatic => f.write_str("com.apple.product-type.library.static"),
			ProductType::Tool => f.write_str("com.apple.product-type.tool"),
		}
	}
}

struct XCConfigurationList {
	pub id: String,
	// isa = XCConfigurationList;
	// buildConfigurations = (
	// 	5DEA4D5D2C43D053008D0969 /* Debug */,
	// 	5DEA4D5E2C43D053008D0969 /* Release */,
	// );
	pub build_configurations: Vec<Key>, // XCBuildConfiguration
	// defaultConfigurationIsVisible = 0;
	pub default_configuration_is_visible: bool,
	// defaultConfigurationName = Release;
	pub default_configuration_name: String,
	// --------------
	pub x_target_type: String,
	pub x_target_name: String,
}

struct XCBuildConfiguration {
	pub id: String,
	// isa = XCBuildConfiguration;
	// buildSettings = {
	// 	CODE_SIGN_STYLE = Automatic;
	// 	EXECUTABLE_PREFIX = lib;
	// 	PRODUCT_NAME = "$(TARGET_NAME)";
	// 	SKIP_INSTALL = YES;
	//  CLANG_CXX_LANGUAGE_STANDARD = "c++20";
	//  GCC_C_LANGUAGE_STANDARD = c17;
	//  GCC_OPTIMIZATION_LEVEL = 2;
	//  STRINGS_FILE_OUTPUT_ENCODING = "UTF-8";
	//  ARCHS = (x86_64,arm64);
	// };
	pub build_settings: BTreeMap<String, BuildSetting>,
	// name = Debug;
	pub name: String,
}

#[derive(Clone)]
enum BuildSetting {
	Single(String),
	Array(Vec<String>),
}

impl BuildSetting {
	fn is_safe_char(c: char) -> bool {
		matches!(c, '0'..='9') | matches!(c, 'A'..='Z') | matches!(c, 'a'..='z') | matches!(c, '_' | '/' | '.')
	}
}

impl core::fmt::Display for BuildSetting {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			BuildSetting::Single(s) => {
				if s.chars().all(BuildSetting::is_safe_char) {
					f.write_str(s)
				} else {
					f.write_char('"')?;
					f.write_str(&s.replace('"', r#"\""#))?;
					f.write_char('"')
				}
			}
			BuildSetting::Array(arr) => {
				if arr.len() == 1 {
					let quoted_item = if arr[0].chars().all(BuildSetting::is_safe_char) {
						&arr[0]
					} else {
						&("\"".to_owned() + &arr[0] + "\"")
					};
					write!(f, "{quoted_item}")
				} else {
					let pad = f.fill();
					let width = f.width().unwrap_or(0);
					f.write_str("(\n")?;
					for element in arr {
						let quoted_item = if element.chars().all(BuildSetting::is_safe_char) {
							element
						} else {
							&("\"".to_owned() + &element.replace('"', r#"\""#) + "\"")
						};
						writeln!(f, "{}{quoted_item},", pad.to_string().repeat(width + 1))?;
					}
					write!(f, "{})", pad.to_string().repeat(width))
				}
			}
		}
	}
}

struct PBXGroup {
	pub id: String,
	pub children: Vec<Key>, // GroupChild[] // PBXGroup or PBXReference
	pub name: Option<String>,
	pub path: Option<String>,
	pub source_tree: String, // Valid values are: "<group>", "<absolute>", SOURCE_ROOT, BUILT_PRODUCTS_DIR, DEVELOPER_DIR, SDKROOT
}

struct BuildPhaseBase {
	pub id: String,
	pub build_action_mask: u32,
	pub files: Vec<Key>,
	pub run_only_for_deployment_postprocessing: bool,
}

type PBXHeadersBuildPhase = BuildPhaseBase;
type PBXSourcesBuildPhase = BuildPhaseBase;
type PBXFrameworksBuildPhase = BuildPhaseBase;
struct PBXCopyFilesBuildPhase {
	base: BuildPhaseBase,
	dst_path: String,
	dst_subfolder_spec: u32,
}

#[derive(Clone)]
enum Reference {
	File(Key),  // PBXFileReference
	Proxy(Key), // PBXReferenceProxy
}

struct PBXBuildFile {
	pub id: String,
	// isa = PBXBuildFile;
	// fileRef = 5D654BEE2C4CCFA3003465E3 /* add.cpp */;
	pub file_ref: Reference,
	// ---------------------
	pub x_name: String,
	pub x_build_phase: String,
}

struct PBXFileReference {
	pub id: String,
	// isa = PBXFileReference;
	// explicitFileType = archive.ar;
	pub file_type: FileRefType,
	// includeInIndex = 0;
	pub include_in_index: Option<bool>,
	// lastKnownFileType = sourcecode.cpp.cpp;
	// pub last_known_file_type: Option<LastKnownFileType>,
	// name = add.cpp
	pub name: Option<String>,
	// path = libexmath.a;
	pub path: String,
	// sourceTree = BUILT_PRODUCTS_DIR;
	pub source_tree: String, // <group> or BUILT_PRODUCTS_DIR ?
}

enum FileRefType {
	Explicit(ExplicitFileType),
	LastKnown(FileType),
}

enum ExplicitFileType {
	Archive,    // archive.ar
	Executable, // compiled.mach-o.executable
}

enum ContainerPortal {
	FileReference(Key),
	Project,
}

impl core::fmt::Display for ExplicitFileType {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			// Why is only compiled.mach-o.executable surrounded in quotes?
			ExplicitFileType::Executable => f.write_str("\"compiled.mach-o.executable\""),
			ExplicitFileType::Archive => f.write_str("archive.ar"),
		}
	}
}

enum FileType {
	Header,    // sourcecode.cpp.h
	C,         // sourcecode.c.c
	Cpp,       // sourcecode.cpp.cpp
	Nasm,      // sourcecode.nasm
	PbProject, // wrapper.pb-project
}

impl core::fmt::Display for FileType {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			FileType::Header => f.write_str("sourcecode.cpp.h"),
			FileType::C => f.write_str("sourcecode.c.c"),
			FileType::Cpp => f.write_str("sourcecode.cpp.cpp"),
			FileType::Nasm => f.write_str("sourcecode.nasm"),
			FileType::PbProject => f.write_str("\"wrapper.pb-project\""),
		}
	}
}

struct PBXTargetDependency {
	pub id: String,
	// isa = PBXTargetDependency;
	// target = 5D654BE92C4CCF39003465E3 /* exmath */;
	pub target: Key, // PBXNativeTarget
	// targetProxy = 5D654BF72C4CD166003465E3 /* PBXContainerItemProxy */;
	pub target_proxy: Key, // PBXContainerItemProxy
}

struct PBXContainerItemProxy {
	pub id: String,
	// isa = PBXContainerItemProxy;
	// containerPortal = 5DEA4D4D2C43D053008D0969 /* Project object */;
	pub container_portal: Key, // PBXFileReference // PBXProject,
	// proxyType = 1;
	pub proxy_type: u32,
	// remoteGlobalIDString = 5D654BE92C4CCF39003465E3;
	pub remote_global_id_string: Key, // PBXFileReference or PBXNativeTarget
	// remoteInfo = exmath;
	pub remote_info: String,
}

struct PBXReferenceProxy {
	pub id: String,
	// isa = PBXReferenceProxy;
	// fileType = "compiled.mach-o.executable";
	pub file_type: ExplicitFileType,
	pub name: Option<String>,
	// path = example;
	pub path: String,
	// remoteRef = 5DF03FB52C70B2E50045F06D /* PBXContainerItemProxy */;
	pub remote_ref: Key, // PBXContainerItemProxy
	// sourceTree = BUILT_PRODUCTS_DIR;
	pub source_tree: String,
}

#[test]
fn test_xcode() {
	use crate::{
		executable::Executable,
		static_library::StaticLibrary, //
		toolchain::Profile,
	};
	use std::path::PathBuf;

	fn diff_at(a: &str, b: &str) -> String {
		let mut line = 1;
		let mut i: usize = 0;
		let mut a_chars = a.chars();
		let mut b_chars = b.chars();
		let mut a_next = a_chars.next();
		while a_next == b_chars.next() {
			i += 1;
			match a_next {
				None => break,
				Some(c) => {
					if c == '\n' {
						line += 1;
					}
				}
			};
			a_next = a_chars.next();
		}
		let a_str: String = a
			.chars()
			.skip(i.saturating_sub(15))
			.take(30)
			.filter(|x| *x != '\n')
			.collect();
		let b_str: String = b
			.chars()
			.skip(i.saturating_sub(15))
			.take(30)
			.filter(|x| *x != '\n')
			.collect();
		format!("line {}:\n{}\n{}\n", line, a_str, b_str)
	}
	use crate::{
		misc::SourcePath,
		toolchain::compiler::{Compiler, ExeLinker},
	};

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
				"11" => Ok("c++11".to_owned()),
				"14" => Ok("c++14".to_owned()),
				"17" => Ok("c++17".to_owned()),
				"20" => Ok("c++20".to_owned()),
				"23" => Ok("c++23".to_owned()),
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

	let project = Arc::new_cyclic(|weak_parent| {
		let exe = Arc::new(Executable {
			parent_project: weak_parent.clone(),
			name: "basic.exe".to_owned(),
			sources: Sources {
				cpp: vec![SourcePath { full: PathBuf::from("main.cpp"), name: "main.cpp".to_owned() }],
				..Default::default()
			},
			links: Vec::new(),
			include_dirs: Vec::new(),
			defines: Vec::new(),
			link_flags: Vec::new(),
			generator_vars: None,
			output_name: None,
		});
		Project {
			info: Arc::new(crate::project::ProjectInfo {
				name: "basic".to_owned(),
				path: PathBuf::from("/path/to/basic"),
			}),
			dependencies: Vec::new(),
			executables: vec![exe],
			static_libraries: Vec::new(),
			object_libraries: Vec::new(),
			interface_libraries: Vec::new(),
		}
	});

	let toolchain = Toolchain {
		msvc_platforms: Vec::new(),
		xcode_platforms: vec!["arm64".to_owned(), "x86_64".to_owned()],
		c_compiler: None,
		cpp_compiler: Some(Box::new(TestCompiler {})),
		nasm_assembler: None,
		static_linker: Some(vec!["llvm-ar".to_owned()]),
		exe_linker: Some(Box::new(TestCompiler {})),
		profile: BTreeMap::<String, Profile>::from([(
			"Debug".to_owned(),
			Profile {
				c_compile_flags: Vec::new(),
				cpp_compile_flags: Vec::new(),
				nasm_assemble_flags: Vec::new(),
				vcxproj: None,
				xcodeproj: Some(XcodeprojectProfile {
					native_target: BTreeMap::from([(
						"STRINGS_FILE_OUTPUT_ENCODING".to_owned(),
						PbxItem::String("UTF-8".to_owned()),
					)]),
					project: BTreeMap::from([(
						"ALWAYS_SEARCH_USER_PATHS".to_owned(),
						PbxItem::String("NO".to_owned()),
					)]),
				}),
			},
		)]),
	};
	let global_opts = GlobalOptions {
		c_standard: Some("17".to_owned()),
		cpp_standard: Some("20".to_owned()),
		position_independent_code: Some(true),
	};
	let xcodeprojs =
		match transform_build_graph_to_xcode_graphs(project.clone(), toolchain, &global_opts, Path::new("")) {
			Err(e) => panic!("{}", e),
			Ok(x) => x,
		};
	let (_, project_nodes) = xcodeprojs.into_iter().next().unwrap();
	let xcodeproj_str = project_nodes.into_string(&project.info.name);
	let expected = include_str!("./xcode_test_01.pbxproj");
	assert_eq!(xcodeproj_str, expected, "{}", diff_at(&xcodeproj_str, expected));
}
