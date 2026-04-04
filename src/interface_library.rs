use std::{
	path::PathBuf,
	sync::{Arc, Weak},
};

use crate::{
	link_type::LinkPtr,
	misc::SourcePath,
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct InterfaceLibrary {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub links: Vec<LinkPtr>,
	pub include_dirs: Vec<SourcePath>,
	pub defines: Vec<String>,
	pub link_flags: Vec<String>,
}

impl Target for InterfaceLibrary {
	fn name(&self) -> &str {
		&self.name
	}
	fn output_name(&self) -> &str {
		&self.name
	}
	fn project(&self) -> Arc<Project> {
		self.parent_project.upgrade().unwrap()
	}
	fn internal_includes(&self) -> Vec<PathBuf> {
		self.public_includes_recursive()
	}
	fn internal_defines(&self) -> Vec<String> {
		self.public_defines_recursive()
	}
	fn internal_link_flags(&self) -> Vec<String> {
		self.public_link_flags_recursive()
	}
	fn internal_links(&self) -> Vec<LinkPtr> {
		self.public_links_recursive()
	}
}

impl LinkTarget for InterfaceLibrary {
	fn public_includes_recursive(&self) -> Vec<PathBuf> {
		let mut includes = crate::misc::index_set::IndexSet::new();
		for link in &self.links {
			for include in link.public_includes_recursive() {
				includes.insert(include);
			}
		}
		for include in self.include_dirs.iter().map(|x| x.full.clone()) {
			includes.insert(include.clone());
		}
		includes.into_iter().collect()
	}
	fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = crate::misc::index_set::IndexSet::new();
		for link in &self.links {
			for def in link.public_defines_recursive() {
				defines.insert(def);
			}
		}
		for def in &self.defines {
			defines.insert(def.clone());
		}
		defines.into_iter().collect()
	}
	fn public_link_flags_recursive(&self) -> Vec<String> {
		let mut flags = crate::misc::index_set::IndexSet::new();
		for link in &self.links {
			for flag in link.public_link_flags_recursive() {
				flags.insert(flag);
			}
		}
		for flag in &self.link_flags {
			flags.insert(flag.clone());
		}
		flags.into_iter().collect()
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

impl InterfaceLibrary {
	pub(crate) fn set_parent(&mut self, parent: Weak<Project>) {
		self.parent_project = parent;
	}
}
