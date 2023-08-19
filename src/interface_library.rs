use std::sync::{Arc, Weak};

use crate::{
	link_type::LinkPtr,
	misc::canonicalize,
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct InterfaceLibrary {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub links: Vec<LinkPtr>,
	pub include_dirs: Vec<String>,
	pub defines: Vec<String>,
	pub link_flags: Vec<String>,
}

impl Target for InterfaceLibrary {
	fn name(&self) -> String {
		self.name.clone()
	}
	fn output_name(&self) -> String {
		self.name.clone()
	}
	fn project(&self) -> Arc<Project> {
		self.parent_project.upgrade().unwrap()
	}
}

impl LinkTarget for InterfaceLibrary {
	fn public_includes(&self) -> Vec<String> {
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		self.include_dirs
			.iter()
			.map(|x| canonicalize(parent_path, x).unwrap())
			.collect()
	}
	fn public_includes_recursive(&self) -> Vec<String> {
		let mut includes = Vec::new();
		for link in &self.links {
			for include in link.public_includes_recursive() {
				if !includes.contains(&include) {
					includes.push(include);
				}
			}
		}
		for include in self.public_includes() {
			if !includes.contains(&include) {
				includes.push(include);
			}
		}
		includes
	}
	fn public_defines(&self) -> Vec<String> {
		self.defines.clone()
	}
	fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = Vec::new();
		for link in &self.links {
			for def in link.public_defines() {
				if !defines.contains(&def) {
					defines.push(def);
				}
			}
		}
		for link in &self.links {
			for def in link.public_defines_recursive() {
				if !defines.contains(&def) {
					defines.push(def);
				}
			}
		}
		for def in &self.defines {
			if !defines.contains(def) {
				defines.push(def.clone());
			}
		}
		defines
	}
	fn public_link_flags(&self) -> Vec<String> {
		self.link_flags.clone()
	}
	fn public_link_flags_recursive(&self) -> Vec<String> {
		let mut flags = Vec::new();
		for link in &self.links {
			for flag in link.public_link_flags_recursive() {
				if !flags.contains(&flag) {
					flags.push(flag);
				}
			}
		}
		for flag in &self.link_flags {
			if !flags.contains(flag) {
				flags.push(flag.clone());
			}
		}
		flags
	}
	fn public_links(&self) -> Vec<LinkPtr> {
		self.links.clone()
	}
	fn public_links_recursive(&self) -> Vec<LinkPtr> {
		let mut links = Vec::new();
		// Bread-first addition
		for link in &self.links {
			links.push(link.clone());
		}
		for link in &self.links {
			links.extend(link.public_links_recursive());
		}
		links
	}
}
