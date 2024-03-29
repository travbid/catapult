use std::{
	path::PathBuf, //
	sync::{Arc, Weak},
};

use crate::{
	link_type::LinkPtr,
	misc::SourcePath,
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct StaticLibrary {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub c_sources: Vec<SourcePath>,
	pub cpp_sources: Vec<SourcePath>,
	pub link_private: Vec<LinkPtr>,
	pub link_public: Vec<LinkPtr>,
	pub include_dirs_public: Vec<SourcePath>,
	pub include_dirs_private: Vec<SourcePath>,
	pub defines_public: Vec<String>,
	pub link_flags_public: Vec<String>,

	pub output_name: Option<String>,
}

impl Target for StaticLibrary {
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

impl LinkTarget for StaticLibrary {
	fn public_includes(&self) -> Vec<PathBuf> {
		self.include_dirs_public.iter().map(|x| x.full.clone()).collect()
	}
	fn public_includes_recursive(&self) -> Vec<PathBuf> {
		let mut includes = Vec::new();
		for link in &self.link_private {
			for include in link.public_includes_recursive() {
				if !includes.contains(&include) {
					includes.push(include);
				}
			}
		}
		for include in self.include_dirs_public.iter().map(|x| &x.full) {
			if !includes.contains(include) {
				includes.push(include.to_owned());
			}
		}
		includes
	}
	fn public_defines(&self) -> Vec<String> {
		self.defines_public.clone()
	}
	fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = Vec::new();
		for link in &self.link_private {
			for def in link.public_defines() {
				if !defines.contains(&def) {
					defines.push(def);
				}
			}
		}
		for link in &self.link_private {
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
		for link in &self.link_private {
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
	fn public_links(&self) -> Vec<LinkPtr> {
		self.link_public.clone()
	}
	fn public_links_recursive(&self) -> Vec<LinkPtr> {
		let mut links = Vec::new();
		// Static libraries have to be linked, even if they're private.
		// The include dirs of the private links won't propagate though.
		// Breadth-first addition
		for link in &self.link_private {
			links.push(link.clone());
		}
		for link in &self.link_public {
			links.push(link.clone());
		}
		for link in &self.link_private {
			links.extend(link.public_links_recursive());
		}
		for link in &self.link_public {
			links.extend(link.public_links_recursive());
		}
		links
	}
}

impl StaticLibrary {
	pub(crate) fn private_includes(&self) -> Vec<PathBuf> {
		self.include_dirs_private.iter().map(|x| x.full.clone()).collect()
	}
	pub(crate) fn private_defines(&self) -> Vec<String> {
		// TODO(Travers)
		Vec::new()
	}
	pub(crate) fn set_parent(&mut self, parent: Weak<Project>) {
		self.parent_project = parent;
	}
}
