mod executable;
pub mod generator;
mod interface_library;
mod link_type;
mod misc;
mod object_library;
pub mod project;
mod starlark_api;
mod starlark_executable;
mod starlark_fmt;
mod starlark_global;
mod starlark_interface_library;
mod starlark_link_target;
mod starlark_object_library;
mod starlark_project;
mod starlark_static_library;
mod static_library;
pub mod target;
pub mod toolchain;

use std::{
	collections::{BTreeMap, HashMap},
	env, fs,
	path::{Path, PathBuf},
	sync::Arc,
	sync::Mutex,
	time::Duration,
};

use anyhow::anyhow;
use base64::Engine;
use flate2::read::GzDecoder;
use reqwest::StatusCode;
use serde::Deserialize;
use starlark::{
	environment::{
		Globals, //
		GlobalsBuilder,
		Module,
	},
	eval::Evaluator,
	syntax::{
		AstModule, //
		Dialect,
		DialectTypes,
	},
};
use tar::Archive;

use project::Project;
use starlark_api::err_msg;
use starlark_global::{PkgOpt, StarGlobal};
use starlark_project::StarProject;
use toolchain::Toolchain;

const CATAPULT_TOML: &str = "catapult.toml";
const BUILD_CATAPULT: &str = "build.catapult";

#[derive(Debug, Deserialize)]
struct Manifest {
	package: PackageManifest,
	dependencies: Option<BTreeMap<String, DependencyManifest>>,
	options: Option<ManifestOptions>,
	package_options: Option<HashMap<String, PkgOpt>>,
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
	name: String,
	// version: Option<String>,
	source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DependencyManifest {
	version: Option<String>,
	registry: Option<String>,
	channel: Option<String>,
	// ---
	path: Option<String>,
	// ---
	git: Option<String>,
	// branch: Option<String>,
	// tag: Option<String>,
	// rev: Option<String>,
	options: Option<HashMap<String, PkgOpt>>,
}

#[derive(Debug, Default, Deserialize)]
struct ManifestOptions {
	c_standard: Option<String>,
	cpp_standard: Option<String>,
	position_independent_code: Option<bool>,
}

#[derive(Debug)]
pub struct GlobalOptions {
	pub c_standard: Option<String>,
	pub cpp_standard: Option<String>,
	pub position_independent_code: Option<bool>,
}

fn read_manifest() -> Result<Manifest, anyhow::Error> {
	let catapult_toml = match fs::read_to_string(CATAPULT_TOML) {
		Ok(x) => x,
		Err(e) => {
			return err_msg(format!("Error opening {}: {}", env::current_dir()?.join(CATAPULT_TOML).display(), e))
		}
	};

	let manifest = match toml::from_str::<Manifest>(&catapult_toml) {
		Ok(x) => x,
		Err(e) => {
			return err_msg(format!("Error reading {}: {}", env::current_dir()?.join(CATAPULT_TOML).display(), e))
		}
	};

	Ok(manifest)
}

pub fn parse_project(toolchain: &Toolchain) -> Result<(Arc<Project>, GlobalOptions), anyhow::Error> {
	let manifest_options = read_manifest()?.options.unwrap_or_default();
	let global_options = GlobalOptions {
		c_standard: manifest_options.c_standard,
		cpp_standard: manifest_options.cpp_standard,
		position_independent_code: manifest_options.position_independent_code,
	};
	let mut combined_deps = BTreeMap::new();
	let project =
		parse_project_inner(".", &global_options, &HashMap::new(), HashMap::new(), toolchain, &mut combined_deps)?;

	match project.into_project() {
		Ok(x) => Ok((x, global_options)),
		Err(e) => Err(anyhow!(e)),
	}
}

#[derive(Deserialize)]
struct PackageRecord {
	// pkg_name: String,
	// version: String,
	hash: String,
	manifest: String,
	recipe: String,
	// datetime_added: i64,
}

fn download_from_registry(
	mut registry: String,
	name: &str,
	info_version: Option<String>,
	info_channel: Option<String>,
) -> Result<PathBuf, anyhow::Error> {
	// Download to tmp dir
	let version = match &info_version {
		Some(x) => x,
		None => return Err(anyhow::anyhow!("Field \"version\" required for dependency \"{}\"", name)),
	};
	let channel = match &info_channel {
		Some(x) => x,
		None => return Err(anyhow::anyhow!("Field \"channel\" required for dependency \"{}\"", name)),
	};
	if !registry.ends_with('/') {
		registry += "/";
	}
	let url = match reqwest::Url::parse(&registry) {
		Ok(x) => x,
		Err(e) => return Err(anyhow::anyhow!(e)),
	};
	let url = match url.join(&("get".to_owned() + "/" + name + "/" + version + "/" + channel)) {
		Ok(x) => x,
		Err(e) => return Err(anyhow::anyhow!(e)),
	};
	println!("Fetching dependency \"{}\" from {} ...", name, url);
	let resp = match reqwest::blocking::Client::builder()
		.build()?
		.get(url.clone())
		.timeout(Duration::from_secs(10))
		.send()
	{
		Ok(resp) => resp,
		Err(err) => return Err(anyhow!("Error trying to fetch \"{}\" from {}:\n    {}", name, url, err)),
	};
	match resp.status() {
		StatusCode::OK => (),
		x => return Err(anyhow!("Request GET \"{}\" returned status {}", url, x)),
	}
	let resp_json = match resp.json::<PackageRecord>() {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};
	let cache_dir = match dirs::cache_dir() {
		Some(x) => x,
		None => return Err(anyhow!("Could not find a HOME directory")),
	};
	let pkg_cache_path = cache_dir.join("catapult").join("cache").join(name).join(channel);
	println!("pkg_cache_path: {:?}", pkg_cache_path);

	let hash_path = pkg_cache_path.join("catapult.hash");
	if let Ok(hash) = fs::read_to_string(&hash_path) {
		if hash.trim() == resp_json.hash.trim() {
			// This package already exists in the cache. Don't download it again.
			log::debug!("Package found in cache. It will not be downloaded: {name}");
			return Ok(pkg_cache_path);
		} else {
			log::info!(
				r#"A cached package was found but its hash does not match the one reported by the registry. It will be re-downloaded.
      Package: {name}
 On-disk hash: {}
Registry hash: {}"#,
				hash.trim(),
				resp_json.hash
			);
		}
	}

	let manifest_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(resp_json.manifest)?;
	let manifest_str = std::str::from_utf8(&manifest_bytes)?;
	let manifest = match toml::from_str::<Manifest>(manifest_str) {
		Ok(x) => x,
		Err(e) => return err_msg(format!("Error reading dependency manifest of {}: {}", name, e)),
	};
	let pkg_source_url = match manifest.package.source {
		Some(x) => x,
		None => return Err(anyhow!("Dependency manifest did not contain source. ({})", name)),
	};
	let src_data_resp = match reqwest::blocking::get(&pkg_source_url) {
		Ok(resp) => resp,
		Err(err) => panic!("Error: {}", err),
	};
	match src_data_resp.status() {
		StatusCode::OK => (),
		x => return Err(anyhow!("Request GET \"{}\" returned status {}", pkg_source_url, x)),
	}
	let tar = GzDecoder::new(src_data_resp);
	let mut archive = Archive::new(tar);
	archive.unpack(&pkg_cache_path)?;

	let manifest_path = pkg_cache_path.join(CATAPULT_TOML);

	match fs::write(manifest_path, manifest_bytes) {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};
	let recipe_path = pkg_cache_path.join(BUILD_CATAPULT);
	let recipe_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(resp_json.recipe)?;
	match fs::write(recipe_path, recipe_bytes) {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};

	match fs::write(hash_path, resp_json.hash.as_bytes()) {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	}

	Ok(pkg_cache_path)
}

fn parse_project_inner<P: AsRef<Path> + ?Sized>(
	src_dir: &P,
	global_options: &GlobalOptions,
	package_options: &HashMap<String, HashMap<String, PkgOpt>>,
	mut pkg_opt_underrides: HashMap<String, PkgOpt>,
	toolchain: &Toolchain,
	dep_map: &mut BTreeMap<String, Arc<StarProject>>,
) -> Result<StarProject, anyhow::Error> {
	let src_dir = src_dir.as_ref();
	let original_dir = match env::current_dir() {
		Ok(x) => x,
		Err(e) => return err_msg(format!("Error getting cwd: {}", e)),
	};

	if let Err(e) = env::set_current_dir(src_dir) {
		return err_msg(format!(
			"Error changing to {} from {}: {}",
			src_dir.to_string_lossy(),
			original_dir.display(),
			e
		));
	}

	let current_dir = match env::current_dir() {
		Ok(x) => x,
		Err(e) => return err_msg(format!("Error getting new cwd: {}", e)),
	};

	let manifest = read_manifest()?;

	if let Some(pkg_opts) = package_options.get(&manifest.package.name) {
		for (opt_name, opt_val) in pkg_opts {
			pkg_opt_underrides.insert(opt_name.clone(), opt_val.clone());
		}
	}
	let mut pkg_opts = package_options.clone();
	pkg_opts.insert(manifest.package.name.clone(), pkg_opt_underrides);

	let mut dependent_projects = Vec::new();

	// Parse dependencies before parsing the dependent
	for (name, info) in manifest.dependencies.unwrap_or(BTreeMap::new()) {
		if let Some(dep_proj) = dep_map.get(&name) {
			dependent_projects.push(dep_proj.clone());
		}

		let pkg_opt_underrides = info.options.unwrap_or_default();

		if let Some(registry) = info.registry {
			let dep_path = download_from_registry(registry, &name, info.version, info.channel)?;
			let dep_proj =
				parse_project_inner(&dep_path, global_options, &pkg_opts, pkg_opt_underrides, toolchain, dep_map)?;
			let dep_proj = Arc::new(dep_proj);
			dependent_projects.push(dep_proj.clone());
			dep_map.insert(name, dep_proj);
		} else if info.git.is_some() {
			// Checkout to tmp dir
			todo!();
		} else if let Some(dep_path) = info.path {
			let dep_proj =
				parse_project_inner(&dep_path, global_options, &pkg_opts, pkg_opt_underrides, toolchain, dep_map)?; //, globals)?;
			let dep_proj = Arc::new(dep_proj);
			dependent_projects.push(dep_proj.clone());
			dep_map.insert(name, dep_proj);
		} else {
			return err_msg("Dependency must specify either \"registry\" or \"git\" or \"path\"".to_owned());
		}

		match env::set_current_dir(&original_dir) {
			Ok(x) => x,
			Err(e) => {
				return err_msg(format!(
					"Error changing to {} from {}: {}",
					original_dir.display(),
					env::current_dir()?.display(),
					e
				))
			}
		};
	}

	let mut option_overrides = manifest.package_options.unwrap_or_default();
	if let Some(pkg_opts) = pkg_opts.get(&manifest.package.name) {
		for (opt_name, opt_val) in pkg_opts {
			log::debug!("Override option: {opt_name}");
			if option_overrides.contains_key(opt_name) {
				option_overrides.insert(opt_name.clone(), opt_val.clone());
			} else {
				log::error!("Package \"{}\" does not provide option \"{opt_name}\"", manifest.package.name);
			}
		}
	}

	let starlark_code = match fs::read_to_string(BUILD_CATAPULT) {
		Ok(x) => x,
		Err(e) => {
			return err_msg(format!("Error reading {}: {}", env::current_dir()?.join(BUILD_CATAPULT).display(), e))
		}
	};
	let this_project = parse_module(
		manifest.package.name.clone(),
		dependent_projects,
		global_options,
		option_overrides,
		toolchain,
		current_dir.to_path_buf(),
		starlark_code,
		// context.clone(),
	)?;

	Ok(this_project)
}

pub(crate) fn setup(
	project: &Arc<Mutex<StarProject>>,
	global_options: &GlobalOptions,
	package_options: HashMap<String, PkgOpt>,
	toolchain: &Toolchain,
) -> Globals {
	let mut globals_builder = GlobalsBuilder::standard();
	starlark::environment::LibraryExtension::Print.add(&mut globals_builder);
	globals_builder.set("GLOBAL", StarGlobal::new(global_options, package_options, toolchain));
	starlark_api::build_api(project, &mut globals_builder);
	globals_builder.build()
}

pub(crate) fn parse_module(
	name: String,
	deps: Vec<Arc<StarProject>>,
	global_options: &GlobalOptions,
	package_options: HashMap<String, PkgOpt>,
	toolchain: &Toolchain,
	current_dir: PathBuf,
	starlark_code: String,
) -> Result<StarProject, anyhow::Error> {
	let dialect = Dialect {
		enable_types: DialectTypes::Enable,
		enable_f_strings: true,
		..Dialect::default()
	};
	let ast = match AstModule::parse(BUILD_CATAPULT, starlark_code, &dialect) {
		Ok(x) => x,
		Err(e) => panic!("{}", e),
	};
	let project_writable = Arc::new(Mutex::new(StarProject::new(name, current_dir, deps.clone())));

	let module = Module::new();
	for dep_proj in deps {
		let proj_value = module.heap().alloc(StarProject::clone(&dep_proj));
		module.set(&dep_proj.name, proj_value);
	}
	let mut eval = Evaluator::new(&module);
	let globals = setup(&project_writable, global_options, package_options, toolchain);
	eval.eval_module(ast, &globals).map_err(|e| e.into_anyhow())?;
	let project = match project_writable.lock() {
		Ok(x) => x.clone(),
		Err(e) => return err_msg(e.to_string()),
	};
	Ok(project)
}
