use core::{cmp, hash};
use std::{
	path::PathBuf, //
	sync::Arc,
};

use crate::{
	interface_library::InterfaceLibrary,
	object_library::ObjectLibrary,
	project::Project,
	shared_library::SharedLibrary,
	static_library::StaticLibrary,
	target::{LinkTarget, Target},
};

#[derive(Clone, Debug)]
pub enum LinkPtr {
	Static(Arc<StaticLibrary>),
	Object(Arc<ObjectLibrary>),
	Interface(Arc<InterfaceLibrary>),
	Shared(Arc<SharedLibrary>),
}

impl cmp::PartialEq for LinkPtr {
	fn eq(&self, other: &LinkPtr) -> bool {
		match (self, other) {
			(Self::Static(a), Self::Static(b)) => {
				core::ptr::eq(Arc::as_ptr(a) as *const (), Arc::as_ptr(b) as *const ())
			}
			(Self::Object(a), Self::Object(b)) => {
				core::ptr::eq(Arc::as_ptr(a) as *const (), Arc::as_ptr(b) as *const ())
			}
			(Self::Interface(a), Self::Interface(b)) => {
				core::ptr::eq(Arc::as_ptr(a) as *const (), Arc::as_ptr(b) as *const ())
			}
			(Self::Shared(a), Self::Shared(b)) => {
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
			Self::Object(x) => (Arc::as_ptr(x) as *const ()).hash(hasher),
			Self::Interface(x) => (Arc::as_ptr(x) as *const ()).hash(hasher),
			Self::Shared(x) => (Arc::as_ptr(x) as *const ()).hash(hasher),
		}
	}
}

impl Target for LinkPtr {
	fn name(&self) -> &str {
		match self {
			Self::Static(x) => x.name(),
			Self::Object(x) => x.name(),
			Self::Interface(x) => x.name(),
			Self::Shared(x) => x.name(),
		}
	}
	fn output_name(&self) -> &str {
		match self {
			Self::Static(x) => x.output_name(),
			Self::Object(x) => x.output_name(),
			Self::Interface(x) => x.output_name(),
			Self::Shared(x) => x.output_name(),
		}
	}
	fn project(&self) -> Arc<Project> {
		match self {
			Self::Static(x) => x.project(),
			Self::Object(x) => x.project(),
			Self::Interface(x) => x.project(),
			Self::Shared(x) => x.project(),
		}
	}
	fn internal_includes(&self) -> Vec<PathBuf> {
		match self {
			Self::Static(x) => x.internal_includes(),
			Self::Object(x) => x.internal_includes(),
			Self::Interface(x) => x.internal_includes(),
			Self::Shared(x) => x.internal_includes(),
		}
	}
	fn internal_defines(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.internal_defines(),
			Self::Object(x) => x.internal_defines(),
			Self::Interface(x) => x.internal_defines(),
			Self::Shared(x) => x.internal_defines(),
		}
	}
	fn internal_link_flags(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.internal_link_flags(),
			Self::Object(x) => x.internal_link_flags(),
			Self::Interface(x) => x.internal_link_flags(),
			Self::Shared(x) => x.internal_link_flags(),
		}
	}
	fn internal_links(&self) -> Vec<LinkPtr> {
		match self {
			Self::Static(x) => x.internal_links(),
			Self::Object(x) => x.internal_links(),
			Self::Interface(x) => x.internal_links(),
			Self::Shared(x) => x.internal_links(),
		}
	}
}

impl LinkTarget for LinkPtr {
	fn public_includes_recursive(&self) -> Vec<PathBuf> {
		match self {
			Self::Static(x) => x.public_includes_recursive(),
			Self::Object(x) => x.public_includes_recursive(),
			Self::Interface(x) => x.public_includes_recursive(),
			Self::Shared(x) => x.public_includes_recursive(),
		}
	}

	fn public_defines_recursive(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_defines_recursive(),
			Self::Object(x) => x.public_defines_recursive(),
			Self::Interface(x) => x.public_defines_recursive(),
			Self::Shared(x) => x.public_defines_recursive(),
		}
	}

	fn public_link_flags_recursive(&self) -> Vec<String> {
		match self {
			Self::Static(x) => x.public_link_flags_recursive(),
			Self::Object(x) => x.public_link_flags_recursive(),
			Self::Interface(x) => x.public_link_flags_recursive(),
			Self::Shared(x) => x.public_link_flags_recursive(),
		}
	}

	fn public_links(&self) -> Vec<LinkPtr> {
		match self {
			Self::Static(x) => x.public_links(),
			Self::Object(x) => x.public_links(),
			Self::Interface(x) => x.public_links(),
			Self::Shared(x) => x.public_links(),
		}
	}

	fn public_links_recursive(&self) -> Vec<LinkPtr> {
		match self {
			Self::Static(x) => x.public_links_recursive(),
			Self::Object(x) => x.public_links_recursive(),
			Self::Interface(x) => x.public_links_recursive(),
			Self::Shared(x) => x.public_links_recursive(),
		}
	}
}
