use core::{
	cmp, //
	fmt,
	hash,
};
use std::sync::Arc;

use crate::{
	link_type::LinkPtr, //
	project::Project,
};

pub trait Target: fmt::Debug + Send + Sync {
	fn name(&self) -> String;
	fn output_name(&self) -> String;
	fn project(&self) -> Arc<Project>;
}

pub trait LinkTarget: Target {
	fn public_includes(&self) -> Vec<String>;
	fn public_includes_recursive(&self) -> Vec<String>;

	fn public_defines(&self) -> Vec<String>;
	fn public_defines_recursive(&self) -> Vec<String>;

	fn public_link_flags(&self) -> Vec<String>;
	fn public_link_flags_recursive(&self) -> Vec<String>;

	fn public_links(&self) -> Vec<LinkPtr>;
	fn public_links_recursive(&self) -> Vec<LinkPtr>;
}

#[derive(Clone)]
pub(super) struct LinkTargetPtr(pub Arc<dyn LinkTarget>);

impl cmp::PartialEq for LinkTargetPtr {
	fn eq(&self, other: &LinkTargetPtr) -> bool {
		core::ptr::eq(Arc::as_ptr(&self.0) as *const (), Arc::as_ptr(&other.0) as *const ())
	}
}
impl cmp::Eq for LinkTargetPtr {}
impl hash::Hash for LinkTargetPtr {
	fn hash<H>(&self, hasher: &mut H)
	where
		H: std::hash::Hasher,
	{
		(Arc::as_ptr(&self.0) as *const ()).hash(hasher)
	}
}
