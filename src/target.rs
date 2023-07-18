use std::{fmt::Debug, sync::Arc};

use crate::project::Project;

pub trait Target: Debug + Send + Sync {
	fn name(&self) -> String;
	fn output_name(&self) -> String;
	fn project(&self) -> Arc<Project>;
}

pub trait LinkTarget: Target {
	fn public_includes_recursive(&self) -> Vec<String>;
	fn public_defines_recursive(&self) -> Vec<String>;
	fn public_link_flags_recursive(&self) -> Vec<String>;
	fn public_links_recursive(&self) -> Vec<Arc<dyn LinkTarget>>;
}
