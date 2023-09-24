use core::fmt;
use std::{
	path::PathBuf,
	sync::{Arc, Weak},
};

use crate::{
	link_type::LinkPtr,
	misc::canonicalize,
	project::Project,
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct Executable {
	pub parent_project: Weak<Project>,

	pub name: String,
	pub c_sources: Vec<String>,
	pub cpp_sources: Vec<String>,
	pub links: Vec<LinkPtr>,
	pub include_dirs: Vec<String>,
	pub defines: Vec<String>,
	pub link_flags: Vec<String>,

	pub output_name: Option<String>,
}

impl fmt::Display for Executable {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Executable{{
   name: {},
   c_sources: [{}],
   cpp_sources: [{}],
   links: [{}],
   include_dirs: [{}],
   defines: [{}],
   link_flags: [{}],
   output_name: {},
}}"#,
			self.name,
			self.c_sources.join(", "),
			self.cpp_sources.join(", "),
			self.links.iter().map(|x| x.name()).collect::<Vec<String>>().join(", "),
			self.include_dirs.join(", "),
			self.defines.join(", "),
			self.link_flags.join(", "),
			self.output_name.clone().unwrap_or("None".to_owned())
		)
	}
}

impl Target for Executable {
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

impl Executable {
	pub(crate) fn public_includes_recursive(&self) -> Vec<PathBuf> {
		let mut includes = Vec::new();
		let parent_path = &self.parent_project.upgrade().unwrap().info.path;
		for link in &self.links {
			for include in link.public_includes_recursive() {
				if !includes.contains(&include) {
					includes.push(include);
				}
			}
		}

		for include in self.include_dirs.iter().map(|x| canonicalize(parent_path, x)) {
			if !includes.contains(&include) {
				includes.push(include);
			}
		}
		includes
	}
	pub(crate) fn public_defines_recursive(&self) -> Vec<String> {
		let mut defines = Vec::new();
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
	pub(crate) fn link_flags_recursive(&self) -> Vec<String> {
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
}
