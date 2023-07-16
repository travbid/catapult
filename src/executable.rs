use std::{
	fmt,
	sync::{Arc, Weak},
};

use crate::{
	project::Project,
	target::{LinkTarget, Target},
};

#[derive(Debug)]
pub struct Executable {
	pub parent_project: Weak<Project>,

	pub name: String,
	pub sources: Vec<String>,
	pub links: Vec<Arc<dyn LinkTarget>>,

	pub output_name: Option<String>,
}

impl fmt::Display for Executable {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			r#"Executable{{
   name: {},
   sources: [{}],
   links: [{}],
}}"#,
			self.name,
			self.sources.join(", "),
			self.links.iter().map(|x| x.name()).collect::<Vec<String>>().join(", ")
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
