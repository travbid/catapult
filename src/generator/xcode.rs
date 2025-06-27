// TODO: Default Target scheme
// TODO: cpp std
// TODO: fPIC
// TODO: uuids
mod compiler;

use core::{cmp, fmt::Write as _, hash};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	fs,
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use starlark::values::OwnedFrozenValue;
use uuid::Uuid;

use crate::{
	executable::Executable,
	link_type::LinkPtr,
	misc::{index_map::IndexMap, thin_ptr::ThinPtr, Sources},
	object_library::ObjectLibrary,
	project::Project,
	starlark_context::{StarContext, StarContextCompiler},
	starlark_generator::eval_vars,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
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
type ProjectMap = IndexMap<ProjectPtr, NodeStore>;
type TargetMap = HashMap<ThinPtr<dyn Target>, Arc<PBXNativeTarget>>;
type MapTargetFileRef = HashMap<ThinPtr<dyn Target>, Arc<PBXFileReference>>; // file_refs
type MapTargetRefProxy = IndexMap<*const Project, IndexMap<ThinPtr<dyn Target>, Arc<PBXReferenceProxy>>>; // ref_proxies
type MapProjectPBX = HashMap<*const Project, Arc<PBXFileReference>>; // xcodeprojs

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

fn generate_xcodeproj(
	project: &Arc<Project>,
	build_dir: &Path,
	toolchain: Toolchain,
	global_opts: GlobalOptions,
) -> Result<(), String> {
	let pbx_projects = transform_build_graph_to_xcode_graphs(project.clone(), toolchain, &global_opts, Path::new(""))?;
	for (subproject, project_nodes) in pbx_projects {
		let xcodeproj_str = project_nodes.into_string(&subproject.0.info.name);
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

struct NodeStore {
	// Order they appear in a .pbxproj
	build_files: Vec<Arc<PBXBuildFile>>,
	container_item_proxies: Vec<Arc<PBXContainerItemProxy>>,
	copy_files_build_phases: Vec<Arc<PBXCopyFilesBuildPhase>>,
	file_references: Vec<Arc<PBXFileReference>>,
	frameworks_build_phases: Vec<Arc<BuildPhaseBase>>,
	groups: Vec<Arc<PBXGroup>>,
	headers_build_phases: Vec<Arc<PBXHeadersBuildPhase>>,
	native_targets: Vec<Arc<PBXNativeTarget>>,
	project: Option<Arc<PBXProject>>,
	reference_proxies: Vec<Arc<PBXReferenceProxy>>,
	source_build_phases: Vec<Arc<PBXSourcesBuildPhase>>,
	target_dependencies: Vec<Arc<PBXTargetDependency>>,
	// xc_build_configurations: Vec<Arc<XCBuildConfiguration>>,
	// xc_configuration_lists: Vec<Arc<XCConfigurationList>>,
}

impl NodeStore {
	fn new(/*project: Arc<PBXProject>*/) -> NodeStore {
		NodeStore {
			build_files: Vec::new(),
			container_item_proxies: Vec::new(),
			copy_files_build_phases: Vec::new(),
			file_references: Vec::new(),
			frameworks_build_phases: Vec::new(),
			groups: Vec::new(),
			headers_build_phases: Vec::new(),
			native_targets: Vec::new(),
			project: None,
			reference_proxies: Vec::new(),
			source_build_phases: Vec::new(),
			target_dependencies: Vec::new(),
			// xc_build_configurations: Vec::new(),
			// xc_configuration_lists: Vec::new(),
		}
	}
	fn new_id(&self, name: &str) -> String {
		thread_local! {
		static  COUNTER: core::cell::Cell<u32> = const { core::cell::Cell::new(1) };
		}

		COUNTER.with(|counter| {
			let ret = format!("{:024X}", counter.get());
			println!("-> {:04X} {name}", counter.get());
			counter.set(counter.get() + 1);
			ret
		})
	}
	fn new_file_reference(
		&mut self,
		file_type: FileRefType,
		include_in_index: Option<bool>,
		name: Option<String>,
		path: String,
		source_tree: String,
	) -> Arc<PBXFileReference> {
		let ret = Arc::new(PBXFileReference {
			id: self.new_id(&("FileReference".to_owned() + name.as_ref().unwrap_or(&String::from("None")))),
			file_type,
			include_in_index,
			name,
			path,
			source_tree,
		});
		self.file_references.push(ret.clone());
		ret
	}
	fn project_targets(
		&mut self,
		project: &Project,
		build_config: &XCConfigurationList,
		build_dir: &Path,
		toolchain: &Toolchain,
		xcodeprojs: &MapProjectPBX,
		file_refs: &mut MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Result<Vec<Arc<PBXNativeTarget>>, String> {
		println!("--- recurse_targets() {} ---", project.info.name);
		let mut ret = Vec::new();
		// let mut target_dependencies: TargetMap<Vec<Arc<PBXTargetDependency>>> = HashMap::new();
		let mut native_targets: TargetMap = HashMap::new();
		// for dependency in &project.dependencies {
		// 	let targets = recurse_targets(dependency, build_config, weak_project);
		// 	ret.extend(targets);
		// }

		for lib in &project.object_libraries {
			let native_target = self.new_native_target_object_lib(
				lib,
				build_config,
				build_dir,
				toolchain,
				xcodeprojs,
				file_refs,
				ref_proxies,
			)?;
			let key = as_thin_key(lib);
			println!("push object nt {:?} {}", key, lib.name);
			native_targets.insert(key, native_target.clone());
			ret.push(native_target);
		}
		for lib in &project.static_libraries {
			let native_target = self.new_native_target_static_lib(
				lib,
				build_config,
				build_dir,
				toolchain,
				xcodeprojs,
				file_refs,
				ref_proxies,
			)?;
			let key = as_thin_key(lib);
			native_targets.insert(key, native_target.clone());
			ret.push(native_target);
		}
		for exe in &project.executables {
			ret.push(self.new_native_target_executable(
				exe,
				build_config,
				// build_dir,
				toolchain,
				xcodeprojs,
				file_refs,
				ref_proxies,
			)?);
		}
		Ok(ret)
	}
	fn new_native_target_object_lib(
		&mut self,
		lib: &Arc<ObjectLibrary>,
		build_config: &XCConfigurationList,
		build_dir: &Path,
		toolchain: &Toolchain,
		xcodeprojs: &MapProjectPBX,
		file_refs: &mut MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Result<Arc<PBXNativeTarget>, String> {
		println!("> new_native_target_object_lib: {}", lib.name);
		let lib_name = "lib".to_owned() + &lib.name + ".a";
		let path = build_dir
			.join(&lib.project().info.name)
			.join(&lib_name)
			.to_string_lossy()
			.to_string();
		let product_reference = self.new_file_reference(
			FileRefType::Explicit(ExplicitFileType::Archive),
			Some(false),
			None,     //Some(path), // name - name and path are reversed
			lib_name, // path
			"BUILT_PRODUCTS_DIR".to_owned(),
		);
		file_refs.insert(as_thin_key(lib), product_reference.clone());

		if let Some(_) = lib.generator_vars.as_ref() {
			return Err("generator_vars are not supported with Xcode generator".to_owned());
			// let gen_sources = self.evaluate_generator_vars(generator_vars, &lib.project().info.path, toolchain)?;
			// &lib.sources.extended_with(gen_sources)
		}
		let include_dirs = lib
			.public_includes_recursive()
			.into_iter()
			.map(|src| src.to_string_lossy().to_string())
			.collect::<Vec<String>>();
		let build_phases = self.add_build_phases(
			&lib.project(),
			&lib.sources,
			&Vec::new(), // &lib.public_links_recursive(),
			xcodeprojs,
			file_refs,
			ref_proxies,
		);
		let build_rules = self.new_build_rules(&lib.sources, toolchain)?;
		Ok(self.new_native_target(
			self.clone_xc_with(build_config, include_dirs),
			build_phases,
			build_rules,
			Vec::new(),
			lib.name().to_owned(),
			lib.name().to_owned(),
			product_reference,
			ProductType::LibraryStatic,
		))
	}

	fn new_native_target_static_lib(
		&mut self,
		lib: &Arc<StaticLibrary>,
		build_config: &XCConfigurationList,
		build_dir: &Path,
		toolchain: &Toolchain,
		xcodeprojs: &MapProjectPBX,
		file_refs: &mut MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Result<Arc<PBXNativeTarget>, String> {
		let lib_name = "lib".to_owned() + &lib.name + ".a";
		let path = build_dir
			.join(&lib.project().info.name)
			.join(&lib_name)
			.to_string_lossy()
			.to_string();
		let product_reference = self.new_file_reference(
			FileRefType::Explicit(ExplicitFileType::Archive),
			Some(false),
			None,     //Some(path), // name - name and path are reversed
			lib_name, // path
			"BUILT_PRODUCTS_DIR".to_owned(),
		);
		file_refs.insert(as_thin_key(lib), product_reference.clone());

		if let Some(_) = lib.generator_vars.as_ref() {
			return Err("generator_vars are not supported with Xcode generator".to_owned());
			// let gen_sources = self.evaluate_generator_vars(generator_vars, &lib.project().info.path, toolchain)?;
			// &lib.sources.extended_with(gen_sources)
		}
		let include_dirs = lib
			.public_includes_recursive()
			.into_iter()
			.map(|src| src.to_string_lossy().to_string())
			.collect::<Vec<String>>();
		let build_phases = self.add_build_phases(
			&lib.project(),
			&lib.sources,
			&Vec::new(), // &lib.public_links_recursive(),
			xcodeprojs,
			file_refs,
			ref_proxies,
		);
		let build_rules = self.new_build_rules(&lib.sources, toolchain)?;
		Ok(self.new_native_target(
			self.clone_xc_with(build_config, include_dirs),
			build_phases,
			build_rules,
			Vec::new(),
			lib.name().to_owned(),
			lib.name().to_owned(),
			product_reference,
			ProductType::LibraryStatic,
		))
	}

	fn new_native_target_executable(
		&mut self,
		exe: &Arc<Executable>,
		build_config: &XCConfigurationList,
		// build_dir: &Path,
		toolchain: &Toolchain,
		xcodeprojs: &MapProjectPBX,
		file_refs: &mut MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Result<Arc<PBXNativeTarget>, String> {
		println!("new_native_target_executable: {}", exe.name);
		let product_reference = self.new_file_reference(
			FileRefType::Explicit(ExplicitFileType::Executable),
			Some(false),
			None,
			exe.name.clone(),
			"BUILT_PRODUCTS_DIR".to_owned(),
		);
		file_refs.insert(as_thin_key(exe), product_reference.clone());

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
		// let exe_links = exe.links_recursive();
		// for exl in &exe_links {
		// 	println!("   exe link: {}", exl.name());
		// }
		let build_phases = self.add_build_phases(
			&exe.project(),
			&exe.sources,
			&exe.links_recursive(),
			xcodeprojs,
			file_refs,
			ref_proxies,
		);
		let build_rules = self.new_build_rules(&exe.sources, toolchain)?;
		Ok(self.new_native_target(
			self.clone_xc_with(build_config, include_dirs),
			build_phases,
			build_rules,
			Vec::new(),
			exe.name().to_owned(),
			exe.name().to_owned(),
			product_reference,
			ProductType::Tool,
		))
	}

	fn new_native_target(
		&mut self,
		build_configuration_list: XCConfigurationList,
		build_phases: Vec<BuildPhase>,
		build_rules: Vec<PBXBuildRule>,
		dependencies: Vec<Arc<PBXTargetDependency>>,
		name: String,
		product_name: String,
		product_reference: Arc<PBXFileReference>,
		product_type: ProductType,
	) -> Arc<PBXNativeTarget> {
		let ret = Arc::new(PBXNativeTarget {
			id: self.new_id(&("native_target: ".to_owned() + &name)),
			build_configuration_list,
			build_phases,
			build_rules,
			dependencies,
			name,
			product_name,
			product_reference,
			product_type,
		});
		self.native_targets.push(ret.clone());
		ret
	}

	fn new_group(
		&mut self,
		children: Vec<GroupChild>,
		name: Option<String>,
		path: Option<String>,
		source_tree: String,
	) -> Arc<PBXGroup> {
		let ret = Arc::new(PBXGroup {
			id: self.new_id(&("Group ".to_owned() + name.as_ref().unwrap_or(&String::from("None")))),
			children,
			name,
			path,
			source_tree,
		});
		self.groups.push(ret.clone());
		ret
	}
	fn add_build_phases(
		&mut self,
		project: &Arc<Project>,
		sources: &Sources,
		links: &Vec<LinkPtr>,
		xcodeprojs: &MapProjectPBX,
		file_refs: &mut MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Vec<BuildPhase> {
		let mut build_phases = Vec::<BuildPhase>::new();
		if !sources.h.is_empty() {
			build_phases.push(BuildPhase::Headers(self.new_headers_build_phase(sources, false)));
		}
		build_phases.push(BuildPhase::Sources(self.new_sources_build_phase(sources, false)));
		build_phases.push(BuildPhase::Frameworks(self.new_frameworks_build_phase(
			links,
			false,
			project,
			xcodeprojs,
			file_refs,
			ref_proxies,
		)));

		// CopyFiles

		build_phases
	}
	fn new_headers_build_phase(
		&mut self,
		sources: &Sources,
		run_only_for_deployment_postprocessing: bool,
	) -> Arc<PBXHeadersBuildPhase> {
		let ret = Arc::new(PBXHeadersBuildPhase {
			id: self.new_id("headers"),
			build_action_mask: 0x7F_FF_FF_FF, // 2147483647
			files: sources
				.h
				.iter()
				.map(|source_path| {
					let reference = self.new_file_reference(
						FileRefType::LastKnown(FileType::Header),
						None,
						Some(source_path.name.clone()),
						source_path.full.to_string_lossy().to_string(),
						"\"<group>\"".to_owned(),
					);
					self.new_build_file(Reference::File(reference))
				})
				.collect(),
			run_only_for_deployment_postprocessing,
		});
		self.headers_build_phases.push(ret.clone());
		ret
	}
	fn new_sources_build_phase(
		&mut self,
		sources: &Sources,
		run_only_for_deployment_postprocessing: bool,
	) -> Arc<PBXSourcesBuildPhase> {
		let ret = Arc::new(BuildPhaseBase {
			id: self.new_id("sources"),
			build_action_mask: 0x7F_FF_FF_FF, //2147483647
			files: sources
				.c
				.iter()
				.map(|source_path| (source_path, FileType::C))
				.chain(sources.cpp.iter().map(|src_pth| (src_pth, FileType::Cpp)))
				.map(|(source_path, last_known_file_type)| {
					let reference = self.new_file_reference(
						FileRefType::LastKnown(last_known_file_type),
						None,
						Some(source_path.name.clone()),
						source_path.full.to_string_lossy().to_string(),
						"\"<group>\"".to_owned(),
					);
					self.new_build_file(Reference::File(reference))
				})
				.collect(),
			run_only_for_deployment_postprocessing,
		});
		self.source_build_phases.push(ret.clone());
		ret
	}
	fn new_frameworks_build_phase(
		&mut self,
		links: &[LinkPtr],
		run_only_for_deployment_postprocessing: bool,
		project: &Arc<Project>,
		xcodeprojs: &MapProjectPBX,
		file_refs: &MapTargetFileRef,
		ref_proxies: &mut MapTargetRefProxy,
	) -> Arc<PBXFrameworksBuildPhase> {
		let ret = Arc::new(PBXFrameworksBuildPhase {
			id: self.new_id("frameworks"),
			build_action_mask: 0x7F_FF_FF_FF, // 2147483647
			files: links
				.iter()
				.filter(|link| match link {
					LinkPtr::Object(_) => true,
					LinkPtr::Static(_) => true,
					LinkPtr::Interface(_) => false,
				})
				.map(|link| {
					let reference = if Arc::ptr_eq(&link.project(), project) {
						match file_refs.get(&link.as_thin_ptr()) {
							Some(file_ref) => Reference::File(file_ref.clone()),
							None => panic!("Could not find PBXFileReference for {}", link.name()), // TODO: Is this correct?
						}
					} else if let Some(proxy_map) = ref_proxies.get_mut(&Arc::as_ptr(&link.project())) {
						let proxy = if let Some(pbx_ref_proxy) = proxy_map.get(&link.as_thin_ptr()) {
							pbx_ref_proxy.clone()
						} else {
							self.add_new_reference_proxy(link, proxy_map, xcodeprojs, file_refs)
						};
						Reference::Proxy(proxy)
					} else {
						let mut proxy_map = IndexMap::new();
						let new_ref_proxy = self.add_new_reference_proxy(link, &mut proxy_map, xcodeprojs, file_refs);
						// ref_proxies.insert(ProjectPtr(link.project().clone()), proxy_map);
						println!(")) ref_proxies.insert {}", link.project().info.name);
						ref_proxies.insert(Arc::as_ptr(&link.project()), proxy_map);
						Reference::Proxy(new_ref_proxy)
					};
					self.new_build_file(reference)
				})
				.collect(),
			run_only_for_deployment_postprocessing,
		});
		self.frameworks_build_phases.push(ret.clone());
		ret
	}
	fn new_build_file(&mut self, file_ref: Reference) -> Arc<PBXBuildFile> {
		let ret = Arc::new(PBXBuildFile { id: self.new_id("build file"), file_ref });
		self.build_files.push(ret.clone());
		ret
	}
	fn add_new_reference_proxy(
		&mut self,
		link: &LinkPtr,
		proxy_map: &mut IndexMap<ThinPtr<dyn Target>, Arc<PBXReferenceProxy>>,
		xcodeprojs: &MapProjectPBX,
		file_refs: &MapTargetFileRef,
	) -> Arc<PBXReferenceProxy> {
		let lib_name = "lib".to_owned() + link.name() + ".a";
		let ret = Arc::new(PBXReferenceProxy {
			id: self.new_id(&("reference proxy ".to_owned() + link.name())),
			file_type: ExplicitFileType::Archive,
			name: None, //Some(
			// 	PathBuf::from(&link.project().info.name)
			// 		.join(&lib_name)
			// 		.to_string_lossy()
			// 		.to_string(),
			// ),
			path: lib_name,
			remote_ref: self.new_container_item_proxy(
				xcodeprojs.get(&Arc::as_ptr(&link.project())).unwrap().clone(),
				2,
				file_refs.get(&link.as_thin_ptr()).unwrap().clone(),
				link.name().to_owned(),
			),
			source_tree: "BUILT_PRODUCTS_DIR".to_owned(),
		});
		proxy_map.insert(link.as_thin_ptr(), ret.clone());
		self.reference_proxies.push(ret.clone());
		println!(")) new ref proxy {} {}", ret.id, link.name());
		ret
	}
	fn new_container_item_proxy(
		&mut self,
		container_portal: Arc<PBXFileReference>,
		proxy_type: u32,
		remote_global_id_string: Arc<PBXFileReference>,
		remote_info: String,
	) -> Arc<PBXContainerItemProxy> {
		let ret = Arc::new(PBXContainerItemProxy {
			id: self.new_id("container item proxy"),
			container_portal,
			proxy_type,
			remote_global_id_string,
			remote_info,
		});
		self.container_item_proxies.push(ret.clone());
		ret
	}
	fn new_xc_configuration_list(
		&self,
		profiles: &BTreeMap<String, XcodeprojectProfile>,
		id_generate: impl Fn() -> String,
	) -> XCConfigurationList {
		XCConfigurationList {
			id: id_generate(), //self.new_id("xc configuration list"),
			build_configurations: profiles
				.iter()
				.map(|(profile_name, profile)| XCBuildConfiguration {
					id: id_generate(), //"-".to_owned(), //self.new_id("xc build configuration"),
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
				})
				.collect(),
			default_configuration_is_visible: false,
			default_configuration_name: profiles.first_key_value().unwrap().0.clone(), // TODO(Travers)
		}
	}
	fn clone_xc_with(&self, xc: &XCConfigurationList, include_dirs: Vec<String>) -> XCConfigurationList {
		XCConfigurationList {
			id: self.new_id("xc config clone"),
			build_configurations: xc
				.build_configurations
				.iter()
				.map(|build_cfg| {
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
					} else {
						build_settings
							.insert("HEADER_SEARCH_PATHS".to_owned(), BuildSetting::Array(include_dirs.clone()));
					}
					XCBuildConfiguration {
						id: self.new_id("xc build clone"),
						build_settings,
						name: build_cfg.name.clone(),
					}
				})
				.collect(),
			default_configuration_is_visible: xc.default_configuration_is_visible,
			default_configuration_name: xc.default_configuration_name.clone(),
		}
	}

	fn evaluate_generator_vars(
		&mut self,
		gen_func: &OwnedFrozenValue,
		project_path: &Path,
		toolchain: &Toolchain,
	) -> Result<Sources, String> {
		let mut gen_sources = Sources::default();
		// let mut build_rules = Vec::new();
		for platform in &toolchain.xcode_platforms {
			let target_triple = map_platform_to_target_triple(platform)?;
			let star_context = StarContext {
				c_compiler: Some(StarContextCompiler { target_triple: target_triple.to_owned() }),
				cpp_compiler: Some(StarContextCompiler { target_triple: target_triple.to_owned() }),
			};
			let generator_vars = eval_vars(gen_func, star_context.clone(), "generator_vars")?;

			let platform_sources = Sources::from_slice(&generator_vars.sources, project_path)?;
			gen_sources = gen_sources.extended_with(platform_sources);
		}
		Ok(gen_sources)
	}

	fn new_build_rules(&self, sources: &Sources, toolchain: &Toolchain) -> Result<Vec<PBXBuildRule>, String> {
		let mut build_rules = Vec::new();
		if !sources.nasm.is_empty() {
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
			build_rules.push(PBXBuildRule {
				id: self.new_id("build rule nasm"),
				compiler_spec: CompilerSpec::ProxyScript,
				file_type: FileType::Nasm,
				// inputFiles = ("$(SRCROOT)/submodules/nasmproj/nasmsrc.asm",);
				// outputFiles = ("$(DERIVED_FILE_DIR)/$(INPUT_FILE_BASE).o",);
				input_files: sources
					.nasm
					.iter()
					.map(|src| src.full.to_string_lossy().to_string())
					.collect(),
				is_editable: true,
				output_files: sources
					.nasm
					.iter()
					.map(|src| format!("$(DERIVED_FILE_DIR)/{}.o", src.name))
					.collect(),
				script: format!("set -x\n{nasm_cmd} -o \"$SCRIPT_OUTPUT_FILE_0\" \"$SCRIPT_INPUT_FILE\"\n"),
			});
		}
		Ok(build_rules)
	}

	fn into_string(self, project_name: &str) -> String {
		// let main_group_uuid = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
		// let project_uuid = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
		// let project_name = &project.info.name;
		// let project_path = &project.info.path.to_string_lossy();
		// let build_styles = toolchain
		// 	.profile
		// 	.iter()
		// 	.map(|(name, profile)| (name.clone(), (Uuid::new_v4().simple().to_string().to_ascii_uppercase(), profile)))
		// 	.collect::<BTreeMap<String, (String, &Profile)>>();

		// let targets = IndexMap::new(); // all_targets(&project);

		let mut project_str = String::new();
		project_str += r#"// !$*UTF8*$!
{
	archiveVersion = 1;
	classes = {
	};
	objectVersion = 56;
	objects = {

"#;

		// project_str += "/* Begin PBXAggregateTarget section */\n";
		// project_str += "/* End PBXAggregateTarget section */\n\n";

		project_str += "/* Begin PBXBuildFile section */\n";
		for native_target in &self.native_targets {
			for build_phase in &native_target.build_phases {
				let files = build_phase.files();
				for file in files {
					project_str += &match &file.file_ref {
						Reference::File(file_ref) => {
							format!(
								"		{} /* {} in {build_phase} */ = {{isa = PBXBuildFile; fileRef = {} /* {} */; }};\n",
								file.id,
								file_ref.name.as_ref().unwrap_or(&file_ref.path),
								file_ref.id,
								file_ref.name.as_ref().unwrap_or(&file_ref.path),
							)
						}
						Reference::Proxy(proxy) => {
							format!(
								"		{} /* {} in {build_phase} */ = {{isa = PBXBuildFile; fileRef = {} /* {} */; }};\n",
								file.id, &proxy.path, proxy.id, proxy.path
							)
						}
					}
				}
			}
		}
		project_str += "/* End PBXBuildFile section */\n\n";

		if self.native_targets.iter().any(|nt| !nt.build_rules.is_empty()) {
			project_str += "/* Begin PBXBuildRules section */\n\n";
			for native_target in self.native_targets.iter() {
				for build_rule in &native_target.build_rules {
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
			}
			project_str += "/* End PBXBuildRules section */\n\n";
		}

		// 		project_str += "/* Begin PBXBuildStyle section */\n";
		// 		for (name, (uuid, _)) in &build_styles {
		// 			project_str += &format!(
		// 				r#"		{uuid} /* {name} */ = {{
		// 			isa = PBXBuildStyle;
		// 			buildSettings = {{
		// 				COPY_PHASE_STRIP = NO;
		// 			}};
		// 			name = {name};
		// 		}};
		// "#
		// 			);
		// 		}
		// 		project_str += "/* End PBXBuildStyle section */\n\n";

		if !self.container_item_proxies.is_empty() {
			project_str += "/* Begin PBXContainerItemProxy section */\n";
			// {
			// let mut seen_items = HashSet::new();
			for proxy in self.container_item_proxies {
				// 		for target_dependency in &native_target.dependencies {
				// 			let id = &target_dependency.target_proxy.id;
				// 			let container_portal = &target_dependency.target_proxy.container_portal.upgrade().unwrap().id;
				// 			let proxy_type = target_dependency.target_proxy.proxy_type;
				// 			let remote_global_id_string = &target_dependency.target_proxy.remote_global_id_string.id;
				// 			let remote_info = &target_dependency.target_proxy.remote_info;
				// 			project_str += &format!(
				// 				r#"		{id} /* PBXContainerItemProxy */ = {{
				// 			isa = PBXContainerItemProxy;
				// 			containerPortal = {container_portal} /* Project object */;
				// 			proxyType = {proxy_type};
				// 			remoteGlobalIDString = {remote_global_id_string};
				// 			remoteInfo = {remote_info};
				// 		}};
				// "#
				// 			);
				// 		}
				// for build_phase in &native_target.build_phases {
				// let frameworks_phase = match &**build_phase {
				// 	BuildPhase::Frameworks(phase) => phase,
				// 	_ => continue,
				// };
				// for file in &frameworks_phase.files {
				// 	let proxy = match &file.file_ref {
				// 		Reference::File(_) => continue,
				// 		Reference::Proxy(proxy) => proxy,
				// 	};
				// 	let key = Arc::as_ptr(&proxy);
				// 	if seen_items.contains(&key) {
				// 		continue;
				// 	}

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
					proxy.container_portal.id,
					proxy.container_portal.name.as_ref().unwrap(),
					proxy.proxy_type,
					proxy.remote_global_id_string.id,
					proxy.remote_info
				);
				// seen_items.insert(key);
				// }
				// 	}
			}
			// }
			project_str += "/* End PBXContainerItemProxy section */\n\n";
		}
		if !self.copy_files_build_phases.is_empty() {
			project_str += "/* Begin PBXCopyFilesBuildPhase section */\n";
			// for native_target in &pbx_project.targets {
			for build_phase in self.copy_files_build_phases {
				// let copyfiles_phase = match *build_phase {
				// 	BuildPhase::CopyFiles(phase) => phase,
				// 	_ => continue,
				// };
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
				for file in &build_phase.base.files {
					let file_ref_path = match &file.file_ref {
						Reference::File(file_ref) => &file_ref.path,
						Reference::Proxy(proxy) => &proxy.path,
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
			// }
			project_str += "/* End PBXCopyFilesBuildPhase section */\n\n";
		}

		project_str += "/* Begin PBXFileReference section */\n";
		for file_ref in self.file_references {
			project_str += &print_pbx_file_reference(&file_ref)
		}
		// {
		// 	let mut file_set = HashSet::new();
		// 	for native_target in &pbx_project.targets {
		// 		println!("- native target: {}", native_target.name);
		// 		for build_phase in &native_target.build_phases {
		// 			let files = build_phase.files();
		// 			println!("  - build_phase: {}", build_phase);
		// 			for file in files {
		// 				let file_ref = match &file.file_ref {
		// 					Reference::File(file_ref) => file_ref,
		// 					Reference::Proxy(proxy) => &proxy.remote_ref.container_portal,
		// 				};
		// 				project_str += &print_pbx_file_reference(file_ref);
		// 				println!("    - file: {}", file_ref.path);
		// 				file_set.insert(Arc::as_ptr(file_ref));
		// 			}
		// 		}
		// 	}
		// 	for native_target in &pbx_project.targets {
		// 		println!("+ native target: {}", native_target.name);
		// 		if !file_set.contains(&Arc::as_ptr(&native_target.product_reference)) {
		// 			let file_ref = &native_target.product_reference;
		// 			println!("    - product_reference: {}", file_ref.path);
		// 			project_str += &print_pbx_file_reference(file_ref);
		// 		}
		// 	}
		// }
		project_str += "/* End PBXFileReference section */\n\n";

		project_str += "/* Begin PBXFrameworksBuildPhase section */\n";
		for build_phase in self.frameworks_build_phases {
			// for build_phase in &native_target.build_phases {
			// let frameworks_phase = match &**build_phase {
			// 	BuildPhase::Frameworks(phase) => phase,
			// 	_ => continue,
			// };
			project_str += &format!(
				r#"		{} /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = {};
			files = (
"#,
				build_phase.id, build_phase.build_action_mask
			);
			for file in &build_phase.files {
				let file_ref_path = match &file.file_ref {
					Reference::File(file_ref) => &file_ref.name.as_ref().unwrap_or(&file_ref.path),
					Reference::Proxy(proxy) => &proxy.path,
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
			// }
		}
		project_str += "/* End PBXFrameworksBuildPhase section */\n\n";

		project_str += "/* Begin PBXGroup section */\n";
		// project_str += &print_pbx_group(&pbx_project.main_group);
		for group in self.groups {
			// let group = match group {
			// 	GroupChild::Group(group) => group,
			// 	GroupChild::Reference(_) => panic!("Unexpected Reference"),
			// };
			project_str += &print_pbx_group(&group);
		}
		// for (group, _) in &pbx_project.project_references {
		// 	project_str += &print_pbx_group(group);
		// }
		project_str += "/* End PBXGroup section */\n\n";

		if !self.headers_build_phases.is_empty() {
			project_str += "/* Begin PBXHeadersBuildPhase section */\n";
			// for native_target in &pbx_project.targets {
			for build_phase in self.headers_build_phases {
				// let headers_phase = match &**build_phase {
				// 	BuildPhase::Headers(phase) => phase,
				// 	_ => continue,
				// };
				// if build_phase.files.is_empty() {
				// 	continue;
				// }
				project_str += &format!(
					r#"		{} /* Headers */ = {{
			isa = PBXHeadersBuildPhase;
			buildActionMask = {};
			files = (
"#,
					build_phase.id, build_phase.build_action_mask
				);
				for file in &build_phase.files {
					let file_ref = match &file.file_ref {
						Reference::File(file_ref) => file_ref,
						Reference::Proxy(_) => panic!(),
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
			// }
			project_str += "/* End PBXHeadersBuildPhase section */\n\n";
		}

		project_str += "/* Begin PBXNativeTarget section */\n";
		for native_target in &self.native_targets {
			project_str += &print_pbx_native_target(native_target);
		}
		project_str += "/* End PBXNativeTarget section */\n\n";

		project_str += "/* Begin PBXProject section */\n";
		let pbx_project = self.project.unwrap();
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
				target_attribute.0.id, target_attribute.1
			);
		}
		project_str += &format!(
			r#"				}};
			}};
			buildConfigurationList = {} /* Build configuration list for PBXProject "{project_name}" */;
			compatibilityVersion = {};
			developmentRegion = {};
			hasScannedForEncodings = {};
			knownRegions = (
"#,
			pbx_project.build_configuration_list.id,
			pbx_project.compatibility_version,
			pbx_project.development_region,
			pbx_project.has_scanned_for_encodings as u8,
		);
		for region in &pbx_project.known_regions {
			project_str += &format!("				{region},\n");
		}
		project_str += &format!(
			r#"			);
			mainGroup = {};
			productRefGroup = {} /* {} */;
			projectDirPath = {};
			projectReferences = (
"#,
			pbx_project.main_group.id,
			pbx_project.product_ref_group.id,
			pbx_project.product_ref_group.name.as_ref().unwrap(),
			pbx_project.project_dir_path,
		);
		for (product_group, project_ref) in &pbx_project.project_references {
			project_str += &format!(
				r#"				{{
					ProductGroup = {} /* Products */;
					ProjectRef = {} /* {} */;
				}},
"#,
				product_group.id,
				project_ref.id,
				project_ref.name.as_ref().unwrap()
			);
		}
		project_str += &format!(
			r#"			);
			projectRoot = "{}";
			targets = (
"#,
			pbx_project.project_root,
		);
		for target in &pbx_project.targets {
			project_str += &format!("				{} /* {} */,\n", target.id, target.name);
		}
		project_str += "			);\n		};\n";
		project_str += "/* End PBXProject section */\n\n";

		if !self.reference_proxies.is_empty() {
			project_str += "/* Begin PBXReferenceProxy section */\n";
			// for project_reference in &pbx_project.project_references {
			for ref_proxy in self.reference_proxies {
				// let ref_proxy = match child {
				// 	GroupChild::Group(_) => panic!("Unexpected PBXGroup"),
				// 	GroupChild::Reference(reference) => match reference {
				// 		Reference::File(_) => panic!("Unexpected PBXFileReference"),
				// 		Reference::Proxy(proxy) => proxy,
				// 	},
				// };
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
					ref_proxy.path, ref_proxy.remote_ref.id, ref_proxy.source_tree
				);
			}
			// }
			project_str += "/* End PBXReferenceProxy section */\n\n";
		}

		project_str += "/* Begin PBXSourcesBuildPhase section */\n";
		// for native_target in &pbx_project.targets {
		for build_phase in self.source_build_phases {
			// let source_phase = match &**build_phase {
			// 	BuildPhase::Sources(phase) => phase,
			// 	_ => continue,
			// };
			project_str += &format!(
				r#"		{} /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = {};
			files = (
"#,
				build_phase.id, build_phase.build_action_mask
			);
			for file in &build_phase.files {
				let file_ref_name = match &file.file_ref {
					Reference::File(file_ref) => &file_ref.name.as_ref().unwrap(),
					Reference::Proxy(_) => panic!("Unexpected PBXReferenceProxy"),
				};
				project_str += &format!("\t\t\t\t{} /* {file_ref_name} in Sources */,\n", file.id);
			}
			project_str += &format!(
				r#"			);
			runOnlyForDeploymentPostprocessing = {};
		}};
"#,
				build_phase.run_only_for_deployment_postprocessing as u8
			);
		}
		// }
		project_str += "/* End PBXSourcesBuildPhase section */\n\n";

		if !self.target_dependencies.is_empty() {
			project_str += "/* Begin PBXTargetDependency section */\n";
			for target_dependency in self.target_dependencies {
				// for target_dependency in &native_target.dependencies {
				project_str += &format!(
					r#"		{} /* PBXTargetDependency */ = {{
			isa = PBXTargetDependency;
			target = {} /* {} */;
			targetProxy = {} /* PBXContainerItemProxy */;
		}};
"#,
					target_dependency.id,
					target_dependency.target.id,
					target_dependency.target.name,
					target_dependency.target_proxy.id
				);
				// }
			}
			project_str += "/* End PBXTargetDependency section */\n\n";
		}

		project_str += "/* Begin XCBuildConfiguration section */\n";
		project_str += &print_xc_build_configuration(&pbx_project.build_configuration_list, "PBXProject", project_name);
		for native_target in &self.native_targets {
			project_str += &print_xc_build_configuration(
				&native_target.build_configuration_list,
				"PBXNativeTarget",
				&native_target.name,
			);
		}
		project_str += "/* End XCBuildConfiguration section */\n\n";

		project_str += "/* Begin XCConfigurationList section */\n";
		project_str += &print_xc_configuration_list(&pbx_project.build_configuration_list, "PBXProject", project_name);
		for native_target in self.native_targets {
			project_str += &print_xc_configuration_list(
				&native_target.build_configuration_list,
				"PBXNativeTarget",
				&native_target.name,
			);
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
}

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

fn print_pbx_group(group: &PBXGroup) -> String {
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
	for child in &group.children {
		match child {
			GroupChild::Reference(reference) => {
				let (id, path) = match reference {
					Reference::File(file_ref) => (&file_ref.id, file_ref.name.as_ref().unwrap_or(&file_ref.path)),
					Reference::Proxy(proxy) => (&proxy.id, &proxy.path),
				};
				ret += &format!("\t\t\t\t{id} /* {path} */,\n");
			}
			GroupChild::Group(group) => {
				if let Some(name) = group.name.as_ref() {
					ret += &format!("\t\t\t\t{} /* {name} */,\n", group.id);
				} else if let Some(path) = group.path.as_ref() {
					ret += &format!("\t\t\t\t{} /* {path} */,\n", group.id);
				} else {
					ret += &format!("\t\t\t\t{},\n", group.id);
				}
			}
		};
		// ret += &format!("\t\t\t\t{id} /* {path} */,\n")
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

fn print_pbx_native_target(native_target: &PBXNativeTarget) -> String {
	let mut ret = format!(
		r#"		{} /* {} */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = {} /* Build configuration list for PBXNativeTarget "{}" */;
			buildPhases = (
"#,
		native_target.id, native_target.name, native_target.build_configuration_list.id, native_target.name
	);
	for build_phase in &native_target.build_phases {
		ret += &format!("				{} /* {build_phase} */,\n", build_phase.id());
	}
	ret += r#"			);
			buildRules = (
"#;
	// for build_rule in &native_target.build_rules {
	// project_str += &format!("				{},\n", build_rule.id);
	// }
	ret += r#"			);
			dependencies = (
"#;
	for dependency in &native_target.dependencies {
		ret += &format!("				{} /* PBXTargetDependency */,\n", dependency.id);
	}
	ret += &format!(
		r#"			);
			name = {};
			productName = {};
			productReference = {} /* {} */;
			productType = "{}";
		}};
"#,
		native_target.name,
		native_target.product_name,
		native_target.product_reference.id,
		native_target
			.product_reference
			.name
			.as_ref()
			.unwrap_or(&native_target.product_reference.path),
		native_target.product_type
	);
	ret
}

fn print_xc_build_configuration(
	build_configuration_list: &XCConfigurationList,
	target_type: &str,
	target_name: &str,
) -> String {
	let mut ret = String::new();
	for build_config in &build_configuration_list.build_configurations {
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
	}
	ret
}

fn print_xc_configuration_list(
	build_configuration_list: &XCConfigurationList,
	target_type: &str,
	target_name: &str,
) -> String {
	let mut ret = format!(
		r#"		{} /* Build configuration list for {} "{}" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
"#,
		build_configuration_list.id, target_type, target_name
	);
	for build_config in &build_configuration_list.build_configurations {
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

// fn all_targets(project: &Project) -> IndexMap {
// 	let mut visited = HashSet::new();
// 	let mut targets = IndexMap::new();
// 	recurse_targets(project, &mut visited, &mut targets);
// 	targets
// }

// fn recurse_targets(project: &Arc<Project>, visited: &mut HashSet<*const Project>, targets: &mut IndexMap) {
// 	visited.insert(Arc::as_ptr(project));

// 	for subproject in &project.dependencies {
// 		if visited.contains(&Arc::as_ptr(subproject)) {
// 			continue;
// 		}
// 		recurse_targets(subproject, visited, targets);
// 	}
// 	for lib in &project.object_libraries {
// 		targets.insert(Uuid::new_v4().to_string().to_ascii_uppercase(), lib.clone());
// 	}
// 	for lib in &project.static_libraries {
// 		targets.insert(Uuid::new_v4().to_string().to_ascii_uppercase(), lib.clone());
// 	}
// 	for lib in &project.interface_libraries {
// 		targets.insert(Uuid::new_v4().to_string().to_ascii_uppercase(), lib.clone());
// 	}
// 	for exe in &project.executables {
// 		targets.insert(Uuid::new_v4().to_string().to_ascii_uppercase(), exe.clone());
// 	}
// 	// ret.extend(project.object_libraries.iter().map(|x| x.clone() as Arc<dyn Target>));
// 	// ret.extend(project.static_libraries.iter().map(|x| x.clone() as Arc<dyn Target>));
// 	// ret.extend(project.interface_libraries.iter().map(|x| x.clone() as Arc<dyn Target>));
// 	// ret.extend(project.executables.iter().map(|x| x.clone() as Arc<dyn Target>));
// 	// ret
// }

// fn recurse_object_lib(lib: &Arc<ObjectLibrary>) -> Vec<Arc<dyn Target>> {
// 	let mut ret: Vec<Arc<dyn Target>> = Vec::new();
// 	for link in &lib.link_private {
// 		let targets = match link {
// 			LinkPtr::Object(obj_lib) => recurse_object_lib(&obj_lib),
// 			LinkPtr::Static(static_lib) => recurse_static_lib(&static_lib),
// 			LinkPtr::Interface(iface_lib) => recurse_interface_lib(&iface_lib),
// 		};
// 		ret.extend(targets);
// 	}
// 	for link in &lib.link_public {
// 		let targets = match link {
// 			LinkPtr::Object(obj_lib) => recurse_object_lib(&obj_lib),
// 			LinkPtr::Static(static_lib) => recurse_static_lib(&static_lib),
// 			LinkPtr::Interface(iface_lib) => recurse_interface_lib(&iface_lib),
// 		};
// 		ret.extend(targets);
// 	}
// 	ret
// }

// fn recurse_static_lib(lib: &Arc<StaticLibrary>) -> Vec<Arc<dyn Target>> {
// 	let mut ret: Vec<Arc<dyn Target>> = Vec::new();
// 	for link in &lib.link_private {
// 		let targets = match link {
// 			LinkPtr::Object(obj_lib) => recurse_object_lib(&obj_lib),
// 			LinkPtr::Static(static_lib) => recurse_static_lib(&static_lib),
// 			LinkPtr::Interface(iface_lib) => recurse_interface_lib(&iface_lib),
// 		};
// 		ret.extend(targets);
// 	}
// 	for link in &lib.link_public {
// 		let targets = match link {
// 			LinkPtr::Object(obj_lib) => recurse_object_lib(&obj_lib),
// 			LinkPtr::Static(static_lib) => recurse_static_lib(&static_lib),
// 			LinkPtr::Interface(iface_lib) => recurse_interface_lib(&iface_lib),
// 		};
// 		ret.extend(targets);
// 	}
// 	ret
// }

// fn recurse_interface_lib(lib: &Arc<InterfaceLibrary>) -> Vec<Arc<dyn Target>> {
// 	let mut ret: Vec<Arc<dyn Target>> = Vec::new();
// 	for link in &lib.links {
// 		let targets = match link {
// 			LinkPtr::Object(obj_lib) => recurse_object_lib(&obj_lib),
// 			LinkPtr::Static(static_lib) => recurse_static_lib(&static_lib),
// 			LinkPtr::Interface(iface_lib) => recurse_interface_lib(&iface_lib),
// 		};
// 		ret.extend(targets);
// 	}
// 	ret
// }
//
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
		for (_, profile) in &mut profiles {
			profile
				.project
				.insert("GCC_C_LANGUAGE_STANDARD".to_owned(), PbxItem::String(compiler.c_std_flag(c_std)?));
		}
	}
	if let Some(cpp_std) = &global_opts.cpp_standard {
		for (_, profile) in &mut profiles {
			profile
				.project
				.insert("CLANG_CXX_LANGUAGE_STANDARD".to_owned(), PbxItem::String(compiler.cpp_std_flag(cpp_std)?));
		}
	}
	let mut projects = ProjectMap::new();
	let mut file_refs = MapTargetFileRef::new();
	transform_graph_inner(project, &profiles, global_opts, build_dir, &toolchain, &mut projects, &mut file_refs)?;
	Ok(projects)
}

fn transform_graph_inner(
	project: Arc<Project>,
	profiles: &BTreeMap<String, XcodeprojectProfile>,
	global_opts: &GlobalOptions,
	build_dir: &Path,
	toolchain: &Toolchain,
	projects: &mut ProjectMap,
	file_refs: &mut MapTargetFileRef,
) -> Result<(), String> {
	let mut node_store = NodeStore::new();
	let project_build_configuration_list = XCConfigurationList {
		id: node_store.new_id("project xc build config"),
		build_configurations: profiles
			.iter()
			.map(|(profile_name, profile)| XCBuildConfiguration {
				id: node_store.new_id("project xc build config"),
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
			})
			.collect(),
		default_configuration_is_visible: false,
		default_configuration_name: profiles.first_key_value().unwrap().0.clone(), // TODO(Travers)
	};
	let native_target_build_configuration_list = XCConfigurationList {
		id: "-".to_owned(),
		build_configurations: profiles
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
			.collect(),
		default_configuration_is_visible: false,
		default_configuration_name: profiles.first_key_value().unwrap().0.clone(), // TODO(Travers)
	};

	let mut xcodeprojs = MapProjectPBX::new();
	for dep in &project.dependencies {
		if projects.contains_key(&ProjectPtr(dep.clone())) {
			continue;
		}
		transform_graph_inner(dep.clone(), profiles, global_opts, build_dir, toolchain, projects, file_refs)?;
		// project.object_libraries.iter().any(|lib| {
		// 	lib.link_private
		// 		.iter()
		// 		.chain(lib.link_public.iter())
		// 		.any(|link| Arc::ptr_eq(&link.project(), dep))
		// });
		let name = dep.info.name.clone() + ".xcodeproj";
		let path = build_dir
			.join("..")
			.join(&dep.info.name)
			.join(&name)
			.to_string_lossy()
			.to_string(); // TODO: Non-relative path?
		xcodeprojs.insert(
			Arc::as_ptr(dep),
			node_store.new_file_reference(
				FileRefType::LastKnown(FileType::PbProject),
				None,
				Some(name),
				path,
				"\"<group>\"".to_owned(),
			), // Arc::new(PBXFileReference {
			   // 	id: new_id(),
			   // 	file_type: FileRefType::LastKnown(LastKnownFileType::PbProject),
			   // 	include_in_index: None,
			   // 	name: Some(name),
			   // 	path,
			   // 	source_tree: "\"<group>\"".to_owned(),
			   // }),
		);
	}
	// let project = Arc::new_cyclic(|weak_project| {
	let mut ref_proxies = MapTargetRefProxy::new();
	let native_targets = node_store.project_targets(
		&project,
		&native_target_build_configuration_list,
		build_dir,
		toolchain,
		&xcodeprojs,
		file_refs,
		&mut ref_proxies,
	)?;
	let product_group = node_store.new_group(
		native_targets
			.iter()
			.map(|target| GroupChild::Reference(Reference::File(target.product_reference.clone())))
			.collect(),
		Some("Products".to_owned()),
		None,
		"\"<group>\"".to_owned(),
	);
	println!("\n+++new frameworks group\n");
	for rp in &ref_proxies {
		let rp =
			rp.1.iter()
				.map(|x| x.1.path.clone())
				.collect::<Vec<String>>()
				.join("\n  ");
		println!("= {rp}");
	}
	let mut unique_file_refs = HashSet::new();
	let frameworks_group = node_store.new_group(
		ref_proxies
			.iter()
			.flat_map(|(_, references)| {
				references
					.iter()
					.map(|(_, ref_proxy)| ref_proxy.remote_ref.container_portal.clone())
			})
			.filter(|item| {
				if unique_file_refs.contains(&Arc::as_ptr(item)) {
					false
				} else {
					unique_file_refs.insert(Arc::as_ptr(item));
					true
				}
			})
			.map(|item| GroupChild::Reference(Reference::File(item)))
			.collect(),
		Some("Frameworks".to_owned()),
		None,
		"\"<group>\"".to_owned(),
	);
	let main_group_children = native_targets
		.iter()
		.rev() // The order matters
		.map(|target| {
			GroupChild::Group(
				node_store.new_group(
					target
						.build_phases
						.iter()
						.filter_map(|phase| match phase {
							BuildPhase::Headers(pbx_phase) | BuildPhase::Sources(pbx_phase) => Some(&pbx_phase.files),
							/* BuildPhase::CopyFiles(_) | */ BuildPhase::Frameworks(_) => None,
						})
						.flat_map(|files| files.iter().map(|file| GroupChild::Reference(file.file_ref.clone())))
						.collect(),
					None,
					Some(target.name.clone()),
					"\"<group>\"".to_owned(),
				),
			)
		})
		.chain(core::iter::once(GroupChild::Group(product_group.clone()))) // Products
		.chain(core::iter::once(GroupChild::Group(frameworks_group))) // Frameworks
		.collect();
	let main_group = node_store.new_group(main_group_children, None, None, "\"<group>\"".to_owned());

	let pbx_project = Arc::new(PBXProject {
		id: node_store.new_id("project"),
		attribute_build_indpendent_targets_in_parallel: true,
		attribute_last_upgrade_check: 1540,
		attribute_target_attributes: native_targets
			.iter()
			.map(|x| (x.clone(), "CreatedOnToolsVersion = 15.4".to_owned()))
			.collect(),
		build_configuration_list: project_build_configuration_list,
		compatibility_version: "\"Xcode 14.0\"".to_owned(),
		development_region: "en".to_owned(),
		has_scanned_for_encodings: false,
		known_regions: vec!["en".to_owned(), "Base".to_owned()],
		main_group,
		product_ref_group: product_group,
		project_dir_path: "\"\"".to_owned(),
		project_references: ref_proxies
			.into_iter()
			.map(|(project_ptr, target_references)|
				// unsafe { (*project_ptr).info.name.clone() },
				(
					unsafe {(*project_ptr).static_libraries.iter().map(|x| target_references.get(&ThinPtr(Arc::as_ptr(x) as *const dyn Target))).collect()},
					// node_store.new_group(
					// 	target_references
					// 		.into_values()
					// 		.map(|reference| GroupChild::Reference(Reference::Proxy(reference)))
					// 		.collect(),
					// 	Some("Products".to_owned()),
					// 	None,
					// 	"\"<group\"".to_owned(),
					// ),
					xcodeprojs.remove(&project_ptr).unwrap(),
				))
			// .collect::<BTreeMap<String, _>>()
			// .into_iter()
			// .map(|(_, val)| val)
			.collect(),
		project_root: String::new(),
		targets: native_targets,
	});
	// });
	node_store.project = Some(pbx_project);

	projects.insert(ProjectPtr(project.clone()), node_store);
	Ok(())
}

fn map_platform_to_target_triple(platform: &str) -> Result<&'static str, String> {
	let platform_target = [
		// clang --version says `arm64-apple-darwin`, Xcode passes `-target arm64-apple-macos` or `-target x86_64-apple-macos`
		// clang doesn't seem to care whether aarch64 or arm64 is used, or whether darwin or macos is used.
		// Use aarch64 to be consistent with the MSVC generator.
		("arm64", "aarch64-apple-darwin"),
		("x86_64", "x86_64-apple-darwin"),
	];
	match platform_target.iter().find(|x| platform == x.0) {
		Some(x) => Ok(x.1),
		None => Err(format!(
			"Unknown platform: {platform}. Known platforms are {}",
			platform_target.map(|x| format!("\"{}\"", x.0)).join(", ")
		)),
	}
}

// fn native_targets_interface_lib(
// 	lib: &Arc<InterfaceLibrary>,
// 	build_config: &XCConfigurationList,
// 	// target_dependencies: &mut TargetMap<Vec<Arc<PBXTargetDependency>>>,
// 	native_targets: &mut TargetMap,
// 	// weak_project: &Weak<PBXProject>,
// ) -> Vec<(Arc<PBXNativeTarget>, *const Project)> {
// 	let mut ret: Vec<(Arc<PBXNativeTarget>, *const Project)> = Vec::new();
// 	for link in &lib.links {
// 		match link {
// 			LinkPtr::Object(object_lib) => match native_targets.get(&link.as_thin_ptr()) {
// 				None => {
// 					let pbx_target = new_native_target_object_lib(
// 						object_lib,
// 						build_config,
// 						// target_dependencies,
// 						// native_targets,
// 						// weak_project,
// 					);
// 					native_targets.insert(link.as_thin_ptr(), pbx_target.clone());
// 					ret.push((pbx_target, Arc::as_ptr(&object_lib.project())));
// 				}
// 				Some(pbx_target) => ret.push((pbx_target.clone(), Arc::as_ptr(&object_lib.project()))),
// 			},
// 			LinkPtr::Static(static_lib) => match native_targets.get(&link.as_thin_ptr()) {
// 				None => {
// 					let pbx_target = new_native_target_static_lib(
// 						static_lib,
// 						build_config,
// 						// target_dependencies,
// 						// native_targets,
// 						// weak_project,
// 					);
// 					native_targets.insert(link.as_thin_ptr(), pbx_target.clone());
// 					ret.push((pbx_target, Arc::as_ptr(&static_lib.project())));
// 				}
// 				Some(pbx_target) => ret.push((pbx_target.clone(), Arc::as_ptr(&static_lib.project()))),
// 			},
// 			LinkPtr::Interface(iface_lib) => {
// 				ret.extend(native_targets_interface_lib(iface_lib, build_config, native_targets));
// 				// let new_native_targets = native_targets_interface_lib(
// 				// 	iface_lib,
// 				// 	build_config,
// 				// 	target_dependencies,
// 				// 	native_targets,
// 				// 	weak_project,
// 				// );
// 				// if new_native_targets.is_empty() {
// 				// 	println!("iface targets empty {}", link.name());
// 				// }
// 				// native_targets.insert(link.as_thin_ptr(), new_native_targets.clone());
// 				// ret.extend(new_native_targets);
// 			}
// 		};
// 	}
// 	if ret.is_empty() {
// 		println!("{} has no links?", lib.name);
// 	}
// 	ret
// }

#[inline]
fn as_thin_key<T: Target>(target: &Arc<T>) -> ThinPtr<dyn Target> {
	ThinPtr(Arc::<T>::as_ptr(target) as *const dyn Target)
}

struct PBXProject {
	pub id: String,
	// isa = PBXProject;
	// attributes = {
	// 	BuildIndependentTargetsInParallel = 1;
	pub attribute_build_indpendent_targets_in_parallel: bool,
	// 	LastUpgradeCheck = 1540;
	pub attribute_last_upgrade_check: u32, // = 1540;
	// 	TargetAttributes = {
	// 		5D654BE92C4CCF39003465E3 = {
	// 			CreatedOnToolsVersion = 15.4;
	// 		};
	// 		5DEA4D542C43D053008D0969 = {
	// 			CreatedOnToolsVersion = 15.4;
	// 		};
	// 	};
	pub attribute_target_attributes: Vec<(Arc<PBXNativeTarget>, String)>,
	// };
	// buildConfigurationList = 5DEA4D502C43D053008D0969 /* Build configuration list for PBXProject "example" */;
	pub build_configuration_list: XCConfigurationList,
	// compatibilityVersion = "Xcode 14.0";
	pub compatibility_version: String,
	// developmentRegion = en;
	pub development_region: String,
	// hasScannedForEncodings = 0;
	pub has_scanned_for_encodings: bool,
	// knownRegions = (en,Base);
	pub known_regions: Vec<String>,
	// mainGroup = 5DEA4D4C2C43D053008D0969;
	pub main_group: Arc<PBXGroup>,
	// productRefGroup = 5DEA4D562C43D053008D0969 /* Products */;
	pub product_ref_group: Arc<PBXGroup>,
	// projectDirPath = "";
	pub project_dir_path: String,
	pub project_references: Vec<(
		Arc<PBXGroup>,         //ProductGroup
		Arc<PBXFileReference>, //ProjectRef
	)>,
	// projectRoot = "";
	pub project_root: String,
	// targets = (
	// 	5DEA4D542C43D053008D0969 /* example */,
	// 	5D654BE92C4CCF39003465E3 /* exmath */,
	// );
	pub targets: Vec<Arc<PBXNativeTarget>>,
}

struct PBXNativeTarget {
	pub id: String,
	// isa = PBXNativeTarget;
	// buildConfigurationList = 5D654BED2C4CCF39003465E3 /* Build configuration list for PBXNativeTarget "exmath" */;
	pub build_configuration_list: XCConfigurationList,
	// buildPhases = (
	// 	5D654BE62C4CCF39003465E3 /* Headers */,
	// 	5D654BE72C4CCF39003465E3 /* Sources */,
	// 	5D654BE82C4CCF39003465E3 /* Frameworks */,
	// );
	pub build_phases: Vec<BuildPhase>,
	// buildRules = ();
	pub build_rules: Vec<PBXBuildRule>,
	// dependencies = ();
	pub dependencies: Vec<Arc<PBXTargetDependency>>,
	// name = exmath;
	pub name: String,
	// productName = exmath;
	pub product_name: String,
	// productReference = 5D654BEA2C4CCF39003465E3 /* libexmath.a */;
	pub product_reference: Arc<PBXFileReference>,
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
	pub build_configurations: Vec<XCBuildConfiguration>,
	// defaultConfigurationIsVisible = 0;
	pub default_configuration_is_visible: bool,
	// defaultConfigurationName = Release;
	pub default_configuration_name: String,
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
		matches!(c, '0'..='9')
			| matches!(c, 'A'..='Z')
			| matches!(c, 'a'..='z')
			| match c {
				'_' | '/' | '.' => true,
				_ => false,
			}
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
	pub children: Vec<GroupChild>,
	pub name: Option<String>,
	pub path: Option<String>,
	pub source_tree: String, // <group> or BUILT_PRODUCTS_DIR ?
}

enum GroupChild {
	Reference(Reference),
	Group(Arc<PBXGroup>),
}

struct BuildPhaseBase {
	pub id: String,
	pub build_action_mask: u32,
	pub files: Vec<Arc<PBXBuildFile>>,
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

enum BuildPhase {
	Headers(Arc<PBXHeadersBuildPhase>),
	Sources(Arc<PBXSourcesBuildPhase>),
	Frameworks(Arc<PBXFrameworksBuildPhase>),
	// CopyFiles(Arc<PBXCopyFilesBuildPhase>),
}

impl BuildPhase {
	fn id(&self) -> &str {
		match self {
			BuildPhase::Headers(phase) | BuildPhase::Sources(phase) | BuildPhase::Frameworks(phase) => &phase.id,
			// BuildPhase::CopyFiles(phase) => &phase.base.id,
		}
	}
	fn files(&self) -> &Vec<Arc<PBXBuildFile>> {
		match self {
			BuildPhase::Headers(phase) | BuildPhase::Sources(phase) | BuildPhase::Frameworks(phase) => &phase.files,
			// BuildPhase::CopyFiles(phase) => &phase.base.files,
		}
	}
}

impl core::fmt::Display for BuildPhase {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			BuildPhase::Headers(_) => f.write_str("Headers"),
			BuildPhase::Sources(_) => f.write_str("Sources"),
			BuildPhase::Frameworks(_) => f.write_str("Frameworks"),
			// BuildPhase::CopyFiles(_) => f.write_str("CopyFiles"),
		}
	}
}

#[derive(Clone)]
enum Reference {
	File(Arc<PBXFileReference>),
	Proxy(Arc<PBXReferenceProxy>),
}

struct PBXBuildFile {
	pub id: String,
	// isa = PBXBuildFile;
	// fileRef = 5D654BEE2C4CCFA3003465E3 /* add.cpp */;
	pub file_ref: Reference,
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
	pub target: Arc<PBXNativeTarget>,
	// targetProxy = 5D654BF72C4CD166003465E3 /* PBXContainerItemProxy */;
	pub target_proxy: PBXContainerItemProxy,
}

struct PBXContainerItemProxy {
	pub id: String,
	// isa = PBXContainerItemProxy;
	// containerPortal = 5DEA4D4D2C43D053008D0969 /* Project object */;
	pub container_portal: Arc<PBXFileReference>, // Weak<PBXProject>,
	// proxyType = 1;
	pub proxy_type: u32,
	// remoteGlobalIDString = 5D654BE92C4CCF39003465E3;
	pub remote_global_id_string: Arc<PBXFileReference>, // or Arc<PBXNativeTarget>
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
	pub remote_ref: Arc<PBXContainerItemProxy>,
	// sourceTree = BUILT_PRODUCTS_DIR;
	pub source_tree: String,
}

#[test]
fn test_xcode() {
	use crate::toolchain::Profile;

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

	let project = Arc::new_cyclic(|weak_parent| {
		let adder_lib = Arc::new(StaticLibrary {
			parent_project: weak_parent.clone(),
			name: "adder".to_owned(),
			sources: Sources {
				cpp: vec![SourcePath { full: PathBuf::from("add.cpp"), name: "add.cpp".to_owned() }],
				h: vec![SourcePath { full: PathBuf::from("add.hpp"), name: "add.hpp".to_owned() }],
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
		});
		Project {
			info: Arc::new(crate::project::ProjectInfo { name: "test_project".to_owned(), path: PathBuf::from(".") }),
			dependencies: Vec::new(),
			executables: vec![Arc::new(Executable {
				parent_project: weak_parent.clone(),
				name: "main".to_owned(),
				sources: Sources {
					cpp: vec![SourcePath { full: PathBuf::from("main.cpp"), name: "main.cpp".to_owned() }],
					..Default::default()
				},
				links: vec![LinkPtr::Static(adder_lib.clone())],
				include_dirs: Vec::new(),
				defines: Vec::new(),
				link_flags: Vec::new(),
				generator_vars: None,
				output_name: None,
			})],
			static_libraries: vec![adder_lib],
			object_libraries: Vec::new(),
			interface_libraries: Vec::new(),
		}
	});
	// let build_dir = PathBuf::from("/home/build");
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
						PbxItem::String("\"UTF-8\"".to_owned()),
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
		c_standard: Some("c17".to_owned()),
		cpp_standard: Some("c++20".to_owned()),
		position_independent_code: Some(true),
	};
	let xcodeprojs =
		match transform_build_graph_to_xcode_graphs(project.clone(), toolchain, &global_opts, Path::new("")) {
			Err(e) => panic!("{}", e),
			Ok(x) => x,
		};
	let (project_ptr, project_nodes) = xcodeprojs.into_iter().next().unwrap();
	assert!(Arc::ptr_eq(&project_ptr.0, &project));
	let xcodeproj_str = project_nodes.into_string(&project.info.name);
	use core::fmt::Write;
	println!(
		"{}",
		xcodeproj_str
			.split('\n')
			.enumerate()
			.fold(String::new(), |mut acc, (n, line)| {
				_ = writeln!(&mut acc, "{}: {}", n + 1, line);
				acc
			}) // .map(|(n, line)| format!("{}: {}\n", n + 1, line))
		    // .collect::<String>()
	);
	let expected = r#"// !$*UTF8*$!
{
	archiveVersion = 1;
	classes = {
	};
	objectVersion = 56;
	objects = {

/* Begin PBXBuildFile section */
		000000000000000000000009 /* add.hpp in Headers */ = {isa = PBXBuildFile; fileRef = 000000000000000000000008 /* add.hpp */; };
		00000000000000000000000C /* add.cpp in Sources */ = {isa = PBXBuildFile; fileRef = 00000000000000000000000B /* add.cpp */; };
		000000000000000000000013 /* main.cpp in Sources */ = {isa = PBXBuildFile; fileRef = 000000000000000000000012 /* main.cpp */; };
		000000000000000000000015 /* test_project/libadder.a in Frameworks */ = {isa = PBXBuildFile; fileRef = 000000000000000000000003 /* test_project/libadder.a */; };
/* End PBXBuildFile section */

/* Begin PBXFileReference section */
		000000000000000000000003 /* test_project/libadder.a */ = {isa = PBXFileReference; explicitFileType = archive.ar; includeInIndex = 0; name = test_project/libadder.a; path = libadder.a; sourceTree = BUILT_PRODUCTS_DIR; };
		000000000000000000000008 /* add.hpp */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.cpp.h; name = add.hpp; path = add.hpp; sourceTree = "<group>"; };
		00000000000000000000000B /* add.cpp */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.cpp.cpp; name = add.cpp; path = add.cpp; sourceTree = "<group>"; };
		000000000000000000000012 /* main.cpp */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.cpp.cpp; name = main.cpp; path = main.cpp; sourceTree = "<group>"; };
		000000000000000000000016 /* test_project/main */ = {isa = PBXFileReference; explicitFileType = "compiled.mach-o.executable"; includeInIndex = 0; name = test_project/main; path = main; sourceTree = BUILT_PRODUCTS_DIR; };
/* End PBXFileReference section */

/* Begin PBXFrameworksBuildPhase section */
		00000000000000000000000D /* Frameworks */ = {
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
		000000000000000000000014 /* Frameworks */ = {
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
				000000000000000000000015 /* libadder.a in Frameworks */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
		000000000000000000000017 /* Products */ = {
			isa = PBXGroup;
			children = (
				000000000000000000000003 /* test_project/libadder.a */,
				000000000000000000000016 /* test_project/main */,
			);
			name = Products;
			sourceTree = "<group>";
		};
		000000000000000000000018 /* Frameworks */ = {
			isa = PBXGroup;
			children = (
			);
			name = Frameworks;
			sourceTree = "<group>";
		};
		000000000000000000000019 /* main */ = {
			isa = PBXGroup;
			children = (
				000000000000000000000012 /* main.cpp */,
			);
			path = main;
			sourceTree = "<group>";
		};
		00000000000000000000001A /* adder */ = {
			isa = PBXGroup;
			children = (
				000000000000000000000008 /* add.hpp */,
				00000000000000000000000B /* add.cpp */,
			);
			path = adder;
			sourceTree = "<group>";
		};
		00000000000000000000001B = {
			isa = PBXGroup;
			children = (
				000000000000000000000019 /* main */,
				00000000000000000000001A /* adder */,
				000000000000000000000017 /* Products */,
				000000000000000000000018 /* Frameworks */,
			);
			sourceTree = "<group>";
		};
/* End PBXGroup section */

/* Begin PBXHeadersBuildPhase section */
		000000000000000000000007 /* Headers */ = {
			isa = PBXHeadersBuildPhase;
			buildActionMask = 2147483647;
			files = (
				000000000000000000000009 /* add.hpp in Headers */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXHeadersBuildPhase section */

/* Begin PBXNativeTarget section */
		000000000000000000000004 /* adder */ = {
			isa = PBXNativeTarget;
			buildConfigurationList = 000000000000000000000005 /* Build configuration list for PBXNativeTarget "adder" */;
			buildPhases = (
				000000000000000000000007 /* Headers */,
				00000000000000000000000A /* Sources */,
				00000000000000000000000D /* Frameworks */,
			);
			buildRules = (
			);
			dependencies = (
			);
			name = adder;
			productName = adder;
			productReference = 000000000000000000000003 /* test_project/libadder.a */;
			productType = "com.apple.product-type.library.static";
		};
		00000000000000000000000E /* main */ = {
			isa = PBXNativeTarget;
			buildConfigurationList = 00000000000000000000000F /* Build configuration list for PBXNativeTarget "main" */;
			buildPhases = (
				000000000000000000000011 /* Sources */,
				000000000000000000000014 /* Frameworks */,
			);
			buildRules = (
			);
			dependencies = (
			);
			name = main;
			productName = main;
			productReference = 000000000000000000000016 /* test_project/main */;
			productType = "com.apple.product-type.tool";
		};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
		00000000000000000000001C /* Project object */ = {
			isa = PBXProject;
			attributes = {
				BuildIndependentTargetsInParallel = 1;
				LastUpgradeCheck = 1540;
				TargetAttributes = {
					000000000000000000000004 = {
						CreatedOnToolsVersion = 15.4;
					};
					00000000000000000000000E = {
						CreatedOnToolsVersion = 15.4;
					};
				};
			};
			buildConfigurationList = 000000000000000000000001 /* Build configuration list for PBXProject "test_project" */;
			compatibilityVersion = "Xcode 14.0";
			developmentRegion = en;
			hasScannedForEncodings = 0;
			knownRegions = (
				en,
				Base,
			);
			mainGroup = 00000000000000000000001B;
			productRefGroup = 000000000000000000000017 /* Products */;
			projectDirPath = "";
			projectReferences = (
			);
			projectRoot = "";
			targets = (
				000000000000000000000004 /* adder */,
				00000000000000000000000E /* main */,
			);
		};
/* End PBXProject section */

/* Begin PBXSourcesBuildPhase section */
		00000000000000000000000A /* Sources */ = {
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
				00000000000000000000000C /* add.cpp in Sources */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
		000000000000000000000011 /* Sources */ = {
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
				000000000000000000000013 /* main.cpp in Sources */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
		000000000000000000000002 /* Debug */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				ALWAYS_SEARCH_USER_PATHS = NO;
			};
			name = Debug;
		};
		000000000000000000000006 /* Debug */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				STRINGS_FILE_OUTPUT_ENCODING = "UTF-8";
			};
			name = Debug;
		};
		000000000000000000000010 /* Debug */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				STRINGS_FILE_OUTPUT_ENCODING = "UTF-8";
			};
			name = Debug;
		};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
		000000000000000000000001 /* Build configuration list for PBXProject "test_project" */ = {
			isa = XCConfigurationList;
			buildConfigurations = (
				000000000000000000000002 /* Debug */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Debug;
		};
		000000000000000000000005 /* Build configuration list for PBXNativeTarget "adder" */ = {
			isa = XCConfigurationList;
			buildConfigurations = (
				000000000000000000000006 /* Debug */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Debug;
		};
		00000000000000000000000F /* Build configuration list for PBXNativeTarget "main" */ = {
			isa = XCConfigurationList;
			buildConfigurations = (
				000000000000000000000010 /* Debug */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Debug;
		};
/* End XCConfigurationList section */
	};
	rootObject = 00000000000000000000001C /* Project object */;
}
"#;
	assert_eq!(xcodeproj_str, expected, "{}", diff_at(&xcodeproj_str, expected));
}
