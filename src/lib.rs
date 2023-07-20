mod executable;
pub mod generator;
mod library;
mod misc;
pub mod project;
mod starlark_api;
mod starlark_executable;
mod starlark_library;
mod starlark_link_target;
mod starlark_project;
mod target;

use std::{
	collections::BTreeMap, //
	env,
	fs,
	path::{Path, PathBuf},
	sync::Arc,
	sync::Mutex,
	time::Duration,
};

use anyhow::anyhow;
use base64::Engine;
use flate2::read::GzDecoder;
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
	},
};
use tar::Archive;

use project::Project;
use starlark_api::err_msg;
use starlark_project::StarProject;

const CATAPULT_TOML: &str = "catapult.toml";
const BUILD_CATAPULT: &str = "build.catapult";

#[derive(Debug, Deserialize)]
struct Manifest {
	package: PackageManifest,
	dependencies: Option<BTreeMap<String, DependencyManifest>>,
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
	name: String,
	version: Option<String>,
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
	branch: Option<String>,
	tag: Option<String>,
	rev: Option<String>,
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

pub fn parse_project() -> Result<Arc<Project>, anyhow::Error> {
	let mut combined_deps = BTreeMap::new();
	let project = parse_project_inner(".", &mut combined_deps)?; //, &globals)?;

	Ok(project.into_project())
}

#[derive(Deserialize)]
struct PackageRecord {
	// pkg_name: String,
	// version: String,
	// hash: String,
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
		// let resp = match reqwest::blocking::get(url) {
		Ok(resp) => resp,
		Err(err) => return Err(anyhow!("Error trying to fetch \"{}\" from {}:\n    {}", name, url, err)),
	};
	let resp_json = match resp.json::<PackageRecord>() {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};
	let cache_path =
		PathBuf::from(env::var("XDG_CONFIG_HOME").or_else(|_| env::var("HOME").map(|home| home + "/.config"))?)
			.join("catapult")
			.join("cache")
			.join(name)
			.join(channel);
	println!("cache_path: {:?}", cache_path);
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
	let src_data_resp = match reqwest::blocking::get(pkg_source_url) {
		Ok(resp) => resp,
		Err(err) => panic!("Error: {}", err),
	};
	let tar = GzDecoder::new(src_data_resp);
	let mut archive = Archive::new(tar);
	archive.unpack(&cache_path)?;

	let manifest_path = cache_path.join("catapult.toml");

	match fs::write(manifest_path, manifest_bytes) {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};
	let recipe_path = cache_path.join("build.catapult");
	let recipe_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(resp_json.recipe)?;
	match fs::write(recipe_path, recipe_bytes) {
		Ok(x) => x,
		Err(e) => return Err(anyhow!(e)),
	};

	Ok(cache_path)
}

fn parse_project_inner<P: AsRef<Path> + ?Sized>(
	src_dir: &P, /*, globals: &Globals*/
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

	let mut dependent_projects = Vec::new();

	// Parse dependencies before parsing the dependent
	for (name, info) in manifest.dependencies.unwrap_or(BTreeMap::new()) {
		if let Some(dep_proj) = dep_map.get(&name) {
			dependent_projects.push(dep_proj.clone());
		}
		if let Some(registry) = info.registry {
			let dep_path = download_from_registry(registry, &name, info.version, info.channel)?;
			let dep_proj = parse_project_inner(&dep_path, dep_map)?;
			let dep_proj = Arc::new(dep_proj);
			dependent_projects.push(dep_proj.clone());
			dep_map.insert(name, dep_proj);
		} else if info.git.is_some() {
			// Checkout to tmp dir
			todo!();
		} else if let Some(dep_path) = info.path {
			let dep_proj = parse_project_inner(&dep_path, dep_map)?; //, globals)?;
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

	let starlark_code = match fs::read_to_string(BUILD_CATAPULT) {
		Ok(x) => x,
		Err(e) => {
			return err_msg(format!("Error reading {}: {}", env::current_dir()?.join(BUILD_CATAPULT).display(), e))
		}
	};
	let this_project = parse_module(
		manifest.package.name.clone(),
		dependent_projects, // &dep_map,
		current_dir.to_path_buf(),
		starlark_code,
		// context.clone(),
	)?;

	Ok(this_project)
}

pub(crate) fn setup(project: &Arc<Mutex<StarProject>>) -> Globals {
	let mut globals_builder = GlobalsBuilder::new();
	starlark_api::build_api(project, &mut globals_builder);
	globals_builder.build()
}

pub(crate) fn parse_module(
	name: String,
	deps: Vec<Arc<StarProject>>,
	current_dir: PathBuf,
	starlark_code: String,
) -> Result<StarProject, anyhow::Error> {
	let ast = match AstModule::parse(BUILD_CATAPULT, starlark_code, &Dialect::Standard) {
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
	let globals = setup(&project_writable);
	eval.eval_module(ast, &globals)?;
	let project = match project_writable.lock() {
		Ok(x) => x.clone(),
		Err(e) => return err_msg(e.to_string()),
	};
	Ok(project)
}

