use core::{cmp, fmt, hash};
use std::{
	path::Path,
	sync::{Arc, Weak},
};

use allocative::Allocative;

use super::{
	link_type::LinkPtr,
	project::Project, //
	starlark_project::StarLinkTargetCache,
};

pub(super) trait StarLinkTarget: Send + Sync + fmt::Debug + Allocative {
	fn as_link_target(
		&self,
		parent: Weak<Project>,
		parent_path: &Path,
		ptr: PtrLinkTarget,
		link_map: &mut StarLinkTargetCache,
	) -> Result<LinkPtr, String>;

	fn name(&self) -> String;
	fn public_includes_recursive(&self) -> Vec<String>;
}

#[derive(Clone)]
pub(super) struct PtrLinkTarget(pub Arc<dyn StarLinkTarget>);

impl cmp::PartialEq for PtrLinkTarget {
	fn eq(&self, other: &PtrLinkTarget) -> bool {
		core::ptr::eq(Arc::as_ptr(&self.0) as *const (), Arc::as_ptr(&other.0) as *const ())
	}
}
impl cmp::Eq for PtrLinkTarget {}
impl hash::Hash for PtrLinkTarget {
	fn hash<H>(&self, hasher: &mut H)
	where
		H: std::hash::Hasher,
	{
		(Arc::as_ptr(&self.0) as *const ()).hash(hasher)
	}
}
