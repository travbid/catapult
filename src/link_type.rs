use core::{cmp, hash};
use std::sync::Arc;

use crate::{
	project::Project,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
};

#[derive(Clone, Debug)]
pub enum LinkPtr {
	Static(Arc<StaticLibrary>),
}

impl cmp::PartialEq for LinkPtr {
	fn eq(&self, other: &LinkPtr) -> bool {
		match (self, other) {
			(Self::Static(a), Self::Static(b)) => {
				core::ptr::eq(Arc::as_ptr(a) as *const (), Arc::as_ptr(b) as *const ())
			}
			_ => false,
		}
	}
}
impl cmp::Eq for LinkPtr {}
impl hash::Hash for LinkPtr {
	fn hash<H>(&self, hasher: &mut H)
	where
		H: std::hash::Hasher,
	{
		match self {
			Self::Static(x) => (Arc::as_ptr(x) as *const ()).hash(hasher),
		}
	}
}

impl Target for LinkPtr {
	fn name(&self) -> String {
		match self {
			Self::Static(x) => x.name(),
		}
	}
	fn output_name(&self) -> String {
		match self {
			Self::Static(x) => x.output_name(),
		}
	}
	fn project(&self) -> Arc<Project> {
		match self {
			Self::Static(x) => x.project(),
		}
	}
}

impl LinkTarget for LinkPtr {
	fn public_includes(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_includes(),
		}
	}

	fn public_includes_recursive(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_includes_recursive(),
		}
	}

	fn public_defines(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.private_defines(),
		}
	}

	fn public_defines_recursive(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_defines_recursive(),
		}
	}

	fn public_link_flags(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_link_flags(),
		}
	}

	fn public_link_flags_recursive(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_link_flags_recursive(),
		}
	}

	fn public_links_recursive(&self) -> Vec<LinkPtr> {
		match self {
			Self::Static(x) => x.public_links_recursive(),
		}
	}
}
