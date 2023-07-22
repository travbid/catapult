use std::sync::{Arc, Weak};

use crate::{
	misc::canonicalize,
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct Library {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub c_sources: Vec<String>,
	pub cpp_sources: Vec<String>,
	pub private_links: Vec<Arc<dyn LinkTarget>>,
	// pub public_links: Vec<Arc<dyn LinkTarget>>,
	pub include_dirs_public: Vec<String>,
	pub include_dirs_private: Vec<String>,
	pub defines_public: Vec<String>,
	pub link_flags_public: Vec<String>,

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

impl LinkTarget for Library {
	fn public_includes(&self) -> Vec<String> {
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		self.include_dirs_public
			.iter()
			.map(|x| canonicalize(parent_path, x).unwrap())
			.collect()
	}
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
			let include = include.unwrap();
			if !includes.contains(&include) {
				includes.push(include);
			}
		}
		includes
	}
	fn public_defines(&self) -> Vec<String> {
		self.defines_public.clone()
	}
	fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = Vec::new();
		for link in &self.private_links {
			for def in link.public_defines() {
				if !defines.contains(&def) {
					defines.push(def);
				}
			}
		}
		for link in &self.private_links {
			for def in link.public_defines_recursive() {
				if !defines.contains(&def) {
					defines.push(def);
				}
			}
		}
		for def in &self.defines_public {
			if !defines.contains(def) {
				defines.push(def.clone());
			}
		}
		defines
	}
	fn public_link_flags(&self) -> Vec<String> {
		self.link_flags_public.clone()
	}
	fn public_link_flags_recursive(&self) -> Vec<String> {
		let mut flags = Vec::new();
		for link in &self.private_links {
			for flag in link.public_link_flags() {
				if !flags.contains(&flag) {
					flags.push(flag);
				}
			}
		}
		// for link in &self.public_links {
		// 	for flag in link.public_link_flags_recursive() {
		// 		if !flags.contains(&flag) {
		// 			flags.push(flag);
		// 		}
		// 	}
		// }
		for flag in &self.link_flags_public {
			if !flags.contains(flag) {
				flags.push(flag.clone());
			}
		}
		flags
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
	pub(crate) fn private_includes(&self) -> Vec<String> {
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		self.include_dirs_private
			.iter()
			.map(|x| canonicalize(parent_path, x).unwrap())
			.collect()
	}
	pub(crate) fn private_defines(&self) -> Vec<String> {
		// TODO(Travers)
		Vec::new()
	}
	pub(crate) fn private_link_flags(&self) -> Vec<String> {
		// TODO(Travers)
		Vec::new()
	}
}

