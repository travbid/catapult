use core::{cmp, fmt, hash};
use std::{
	collections::HashMap, //
	path::Path,
	sync::{Arc, Weak},
};

use allocative::Allocative;
use starlark::values::OwnedFrozenValue;

use super::{
	link_type::LinkPtr,
	project::Project, //
	starlark_interface_library::StarIfaceLibrary,
	starlark_object_library::StarObjectLibrary,
	starlark_project::StarLinkTargetCache,
	starlark_shared_library::StarSharedLibrary,
	starlark_static_library::StarStaticLibrary,
};

pub(super) trait StarLinkTarget: Send + Sync + fmt::Debug + Allocative {
	fn as_link_target(
		&self,
		parent: Weak<Project>,
		parent_path: &Path,
		ptr: PtrLinkTarget,
		link_map: &mut StarLinkTargetCache,
		gen_name_map: &HashMap<String, OwnedFrozenValue>,
	) -> Result<LinkPtr, String>;

	fn name(&self) -> String;
	fn public_includes_recursive(&self) -> Vec<String>;
}

#[derive(Allocative, Clone, Debug)]
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

#[derive(Clone, Debug, Allocative)]
pub(super) enum StarLinkTargetRef {
	Static(Arc<StarStaticLibrary>),
	Object(Arc<StarObjectLibrary>),
	Interface(Arc<StarIfaceLibrary>),
	Shared(Arc<StarSharedLibrary>),
}

impl StarLinkTargetRef {
	pub(super) fn name(&self) -> &str {
		match self {
			Self::Static(x) => &x.name,
			Self::Object(x) => &x.name,
			Self::Interface(x) => &x.name,
			Self::Shared(x) => &x.name,
		}
	}

	pub(super) fn as_ptr_link_target(&self) -> PtrLinkTarget {
		match self {
			Self::Static(x) => PtrLinkTarget(x.clone()),
			Self::Object(x) => PtrLinkTarget(x.clone()),
			Self::Interface(x) => PtrLinkTarget(x.clone()),
			Self::Shared(x) => PtrLinkTarget(x.clone()),
		}
	}
}
