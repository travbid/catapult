use std::{
	path::{Path, PathBuf},
	sync::{Arc, Weak},
};

use crate::{
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct Library {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub sources: Vec<String>,
	pub private_links: Vec<Arc<dyn LinkTarget>>,
	pub include_dirs_public: Vec<String>,
	pub include_dirs_private: Vec<String>,
	pub output_name: Option<String>,
}

impl Target for Library {
	fn name(&self) -> String {
		self.name.clone()
	}
	fn output_name(&self) -> String {
		match &self.output_name {
			Some(output_name) => output_name.clone(),
			None => self.name.clone(),
		}
	}
	fn project(&self) -> Arc<Project> {
		self.parent_project.upgrade().unwrap()
	}
}

fn canonicalize(parent_path: &Path, x: &String) -> String {
	let path = PathBuf::from(x);
	if path.is_absolute() {
		x.to_owned()
	} else {
		parent_path.join(x).canonicalize().unwrap().to_str().unwrap().to_owned()
	}
}

impl LinkTarget for Library {
	fn public_includes_recursive(&self) -> Vec<String> {
		let mut includes = Vec::new();
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		for link in &self.private_links {
			for include in link.public_includes_recursive() {
				if !includes.contains(&include) {
					includes.push(include);
				}
			}
		}
		for include in self.include_dirs_public.iter().map(|x| canonicalize(parent_path, x)) {
			if !includes.contains(&include) {
				includes.push(include);
			}
		}
		includes
	}
	fn private_includes(&self) -> Vec<String> {
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		self.include_dirs_private
			.iter()
			.map(|x| canonicalize(parent_path, x))
			.collect()
	}
	fn public_links_recursive(&self) -> Vec<Arc<dyn LinkTarget>> {
		let mut links: Vec<Arc<dyn LinkTarget>> = vec![];
		for link in &self.private_links {
			links.extend(link.public_links_recursive());
		}
		links
	}
}

impl Library {
}

