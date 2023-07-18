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
	path::PathBuf,
	sync::Arc,
	sync::Mutex,
};

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

fn parse_project_inner(
	src_dir: &str, /*, globals: &Globals*/
	dep_map: &mut BTreeMap<String, Arc<StarProject>>,
) -> Result<StarProject, anyhow::Error> {
	let original_dir = match env::current_dir() {
		Ok(x) => x,
		Err(e) => return err_msg(format!("Error getting cwd: {}", e)),
	};

	if let Err(e) = env::set_current_dir(src_dir) {
		return err_msg(format!("Error changing to {} from {}: {}", &src_dir, original_dir.display(), e));
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
		if info.registry.is_some() {
			// Download to tmp dir
			todo!();
		} else if info.git.is_some() {
			// Checkout to tmp dir
			todo!();
		} else if info.path.is_some() {
			let dep_proj = parse_project_inner(&info.path.unwrap(), dep_map)?; //, globals)?;
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

