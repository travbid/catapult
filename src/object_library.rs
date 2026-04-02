use std::{
	path::PathBuf, //
	sync::{Arc, Weak},
};

use starlark::values::OwnedFrozenValue;

use crate::{
	link_type::LinkPtr,
	misc::{SourcePath, Sources},
	project::Project, //
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct ObjectLibrary {
	pub parent_project: Weak<Project>,
	pub name: String,
	pub sources: Sources,
	pub link_private: Vec<LinkPtr>,
	pub link_public: Vec<LinkPtr>,
	pub include_dirs_private: Vec<SourcePath>,
	pub include_dirs_public: Vec<SourcePath>,
	pub defines_private: Vec<String>,
	pub defines_public: Vec<String>,
	pub link_flags_public: Vec<String>,

	pub generator_vars: Option<OwnedFrozenValue>,

	pub output_name: Option<String>,
}

impl Target for ObjectLibrary {
	fn name(&self) -> &str {
		&self.name
	}
	fn output_name(&self) -> &str {
		match &self.output_name {
			Some(output_name) => output_name,
			None => &self.name,
		}
	}
	fn project(&self) -> Arc<Project> {
		self.parent_project.upgrade().unwrap()
	}
}

impl LinkTarget for ObjectLibrary {
	fn public_includes(&self) -> Vec<PathBuf> {
		self.include_dirs_public.iter().map(|x| x.full.clone()).collect()
	}
	fn public_includes_recursive(&self) -> Vec<PathBuf> {
		let mut includes = crate::misc::index_set::IndexSet::new();
		for link in &self.link_public {
			for include in link.public_includes_recursive() {
				includes.insert(include);
			}
		}
		for include in &self.include_dirs_public {
			includes.insert(include.full.clone());
		}
		includes.into_iter().collect()
	}
	fn public_defines(&self) -> Vec<String> {
		self.defines_public.clone()
	}
	fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = crate::misc::index_set::IndexSet::new();
		for link in &self.link_public {
			for def in link.public_defines_recursive() {
				defines.insert(def);
			}
		}
		for def in &self.defines_public {
			defines.insert(def.clone());
		}
		defines.into_iter().collect()
	}
	fn public_link_flags(&self) -> Vec<String> {
		self.link_flags_public.clone()
	}
	fn public_link_flags_recursive(&self) -> Vec<String> {
		let mut flags = crate::misc::index_set::IndexSet::new();
		for link in &self.link_public {
			for flag in link.public_link_flags_recursive() {
				flags.insert(flag);
			}
		}
		for flag in &self.link_flags_public {
			flags.insert(flag.clone());
		}
		flags.into_iter().collect()
	}
	fn public_links(&self) -> Vec<LinkPtr> {
		self.link_public.clone()
	}
	fn public_links_recursive(&self) -> Vec<LinkPtr> {
		let mut links = Vec::new();
		// Object libraries have to be linked, even if they're private.
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

impl ObjectLibrary {
	pub(crate) fn internal_includes(&self) -> Vec<PathBuf> {
		let mut includes = crate::misc::index_set::IndexSet::new();
		for include in self.public_includes_recursive() {
			includes.insert(include);
		}
		for include in self.private_includes() {
			includes.insert(include);
		}
		for link in &self.link_private {
			for include in link.public_includes_recursive() {
				includes.insert(include);
			}
		}
		includes.into_iter().collect()
	}
	pub(crate) fn internal_defines(&self) -> Vec<String> {
		let mut defines = crate::misc::index_set::IndexSet::new();
		for def in self.public_defines_recursive() {
			defines.insert(def);
		}
		for def in self.private_defines() {
			defines.insert(def.clone());
		}
		for link in &self.link_private {
			for def in link.public_defines_recursive() {
				defines.insert(def);
			}
		}
		defines.into_iter().collect()
	}
	pub(crate) fn private_includes(&self) -> Vec<PathBuf> {
		self.include_dirs_private.iter().map(|x| x.full.clone()).collect()
	}
	pub(crate) fn private_defines(&self) -> &[String] {
		&self.defines_private
	}
	pub(crate) fn set_parent(&mut self, parent: Weak<Project>) {
		self.parent_project = parent;
	}
}
