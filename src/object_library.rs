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
	fn internal_includes(&self) -> Vec<PathBuf> {
		let mut includes = crate::misc::index_set::IndexSet::new();
		for include in self.public_includes_recursive() {
			includes.insert(include);
		}
		for include in self.include_dirs_private.iter().map(|x| x.full.clone()) {
			includes.insert(include);
		}
		for link in &self.link_private {
			for include in link.public_includes_recursive() {
				includes.insert(include);
			}
		}
		includes.into_iter().collect()
	}
	fn internal_defines(&self) -> Vec<String> {
		let mut defines = crate::misc::index_set::IndexSet::new();
		for def in self.public_defines_recursive() {
			defines.insert(def);
		}
		for def in &self.defines_private {
			defines.insert(def.clone());
		}
		for link in &self.link_private {
			for def in link.public_defines_recursive() {
				defines.insert(def);
			}
		}
		defines.into_iter().collect()
	}
	fn internal_link_flags(&self) -> Vec<String> {
		self.public_link_flags_recursive()
	}
	fn internal_links(&self) -> Vec<LinkPtr> {
		self.public_links_recursive()
	}
}

impl LinkTarget for ObjectLibrary {
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
	pub(crate) fn set_parent(&mut self, parent: Weak<Project>) {
		self.parent_project = parent;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::static_library::StaticLibrary;

	fn new_static_lib(
		weak_parent: &Weak<Project>,
		name: &str,
		priv_links: &[&Arc<StaticLibrary>],
		pub_links: &[&Arc<StaticLibrary>],
		inc: &str,
		def: &str,
	) -> Arc<StaticLibrary> {
		Arc::new(StaticLibrary {
			parent_project: weak_parent.clone(),
			name: name.to_owned(),
			sources: Sources::default(),
			link_private: priv_links.iter().map(|x| LinkPtr::Static((*x).clone())).collect(),
			link_public: pub_links.iter().map(|x| LinkPtr::Static((*x).clone())).collect(),
			include_dirs_private: vec![SourcePath {
				full: PathBuf::from("private/include"),
				name: "private/include".to_owned(),
			}],
			include_dirs_public: vec![SourcePath { full: PathBuf::from(inc), name: inc.to_owned() }],
			defines_private: vec!["PRIVATE_DEF".to_owned()],
			defines_public: vec![def.to_owned()],
			link_flags_public: Vec::new(),
			generator_vars: None,
			output_name: None,
		})
	}

	#[test]
	fn test_internal_properties() {
		let project = Arc::new_cyclic(|weak_parent| {
			let leaf_shared =
				new_static_lib(weak_parent, "leaf_shared", &[], &[], "leaf_shared_inc", "LEAF_SHARED_DEF");
			let leaf_priv_priv =
				new_static_lib(weak_parent, "leaf_priv_priv", &[], &[], "leaf_priv_priv_inc", "LEAF_PRIV_PRIV_DEF");
			let leaf_priv_pub =
				new_static_lib(weak_parent, "leaf_priv_pub", &[], &[], "leaf_priv_pub_inc", "LEAF_PRIV_PUB_DEF");
			let leaf_pub_priv =
				new_static_lib(weak_parent, "leaf_pub_priv", &[], &[], "leaf_pub_priv_inc", "LEAF_PUB_PRIV_DEF");
			let leaf_pub_pub =
				new_static_lib(weak_parent, "leaf_pub_pub", &[], &[], "leaf_pub_pub_inc", "LEAF_PUB_PUB_DEF");
			let mid_priv = new_static_lib(
				weak_parent,
				"mid_priv",
				&[&leaf_priv_priv],
				&[&leaf_priv_pub, &leaf_shared],
				"mid_priv_inc",
				"MID_PRIV_DEF",
			);
			let mid_pub = new_static_lib(
				weak_parent,
				"mid_pub",
				&[&leaf_pub_priv],
				&[&leaf_pub_pub, &leaf_shared],
				"mid_pub_inc",
				"MID_PUB_DEF",
			);
			let main_lib = Arc::new(ObjectLibrary {
				parent_project: weak_parent.clone(),
				name: "main_lib".to_owned(),
				sources: Sources::default(),
				link_private: vec![LinkPtr::Static(mid_priv.clone())],
				link_public: vec![LinkPtr::Static(mid_pub.clone())],
				include_dirs_private: vec![SourcePath {
					full: PathBuf::from("main_priv_inc"),
					name: "main_priv_inc".to_owned(),
				}],
				include_dirs_public: vec![SourcePath {
					full: PathBuf::from("main_pub_inc"),
					name: "main_pub_inc".to_owned(),
				}],
				defines_private: vec!["MAIN_PRIV_DEF".to_owned()],
				defines_public: vec!["MAIN_PUB_DEF".to_owned()],
				link_flags_public: Vec::new(),
				generator_vars: None,
				output_name: None,
			});
			Project {
				info: Arc::new(crate::project::ProjectInfo { name: "test".to_owned(), path: PathBuf::from(".") }),
				dependencies: Vec::new(),
				executables: Vec::new(),
				link_targets: vec![
					LinkPtr::Object(main_lib.clone()),
					LinkPtr::Static(leaf_shared.clone()),
					LinkPtr::Static(leaf_pub_pub.clone()),
					LinkPtr::Static(leaf_pub_priv.clone()),
					LinkPtr::Static(leaf_priv_pub.clone()),
					LinkPtr::Static(leaf_priv_priv.clone()),
					LinkPtr::Static(mid_pub.clone()),
					LinkPtr::Static(mid_priv.clone()),
				],
			}
		});

		let main_lib = project.link_targets.iter().find(|x| x.name() == "main_lib").unwrap();

		let internal_includes = main_lib.internal_includes();

		// Direct properties
		assert!(internal_includes.contains(&PathBuf::from("main_pub_inc")));
		assert!(internal_includes.contains(&PathBuf::from("main_priv_inc")));

		// Properties from public links
		assert!(internal_includes.contains(&PathBuf::from("mid_pub_inc")));
		assert!(internal_includes.contains(&PathBuf::from("leaf_pub_pub_inc")));

		// Properties from private links
		assert!(internal_includes.contains(&PathBuf::from("mid_priv_inc")));
		assert!(internal_includes.contains(&PathBuf::from("leaf_priv_pub_inc")));

		// Properties blocked by private link boundaries
		assert!(!internal_includes.contains(&PathBuf::from("leaf_pub_priv_inc")));
		assert!(!internal_includes.contains(&PathBuf::from("leaf_priv_priv_inc")));

		// Shared properties
		assert!(internal_includes.contains(&PathBuf::from("leaf_shared_inc")));
		assert_eq!(internal_includes.len(), 7, "Includes should be deduplicated");

		let public_includes = main_lib.public_includes_recursive();

		// Direct public properties
		assert!(public_includes.contains(&PathBuf::from("main_pub_inc")));
		assert!(!public_includes.contains(&PathBuf::from("main_priv_inc")));

		// Properties from public links
		assert!(public_includes.contains(&PathBuf::from("mid_pub_inc")));
		assert!(public_includes.contains(&PathBuf::from("leaf_pub_pub_inc")));

		// Properties blocked by main_lib's own private link boundary
		assert!(!public_includes.contains(&PathBuf::from("mid_priv_inc")));
		assert!(!public_includes.contains(&PathBuf::from("leaf_priv_pub_inc")));

		// Properties blocked by transitive private link boundaries
		assert!(!public_includes.contains(&PathBuf::from("leaf_pub_priv_inc")));
		assert!(!public_includes.contains(&PathBuf::from("leaf_priv_priv_inc")));

		// Shared properties
		assert!(public_includes.contains(&PathBuf::from("leaf_shared_inc")));
		assert_eq!(public_includes.len(), 4, "Includes should be deduplicated");

		let internal_defines = main_lib.internal_defines();

		// Direct properties
		assert!(internal_defines.contains(&"MAIN_PUB_DEF".to_owned()));
		assert!(internal_defines.contains(&"MAIN_PRIV_DEF".to_owned()));

		// Properties from public links
		assert!(internal_defines.contains(&"MID_PUB_DEF".to_owned()));
		assert!(internal_defines.contains(&"LEAF_PUB_PUB_DEF".to_owned()));

		// Properties from private links
		assert!(internal_defines.contains(&"MID_PRIV_DEF".to_owned()));
		assert!(internal_defines.contains(&"LEAF_PRIV_PUB_DEF".to_owned()));

		// Properties blocked by private link boundaries
		assert!(!internal_defines.contains(&"LEAF_PUB_PRIV_DEF".to_owned()));
		assert!(!internal_defines.contains(&"LEAF_PRIV_PRIV_DEF".to_owned()));

		// Shared properties
		assert!(internal_defines.contains(&"LEAF_SHARED_DEF".to_owned()));
		assert_eq!(internal_defines.len(), 7, "Defines should be deduplicated");

		let public_defines = main_lib.public_defines_recursive();

		// Direct public properties
		assert!(public_defines.contains(&"MAIN_PUB_DEF".to_owned()));
		assert!(!public_defines.contains(&"MAIN_PRIV_DEF".to_owned()));

		// Properties from public links
		assert!(public_defines.contains(&"MID_PUB_DEF".to_owned()));
		assert!(public_defines.contains(&"LEAF_PUB_PUB_DEF".to_owned()));

		// Properties blocked by main_lib's own private link boundary
		assert!(!public_defines.contains(&"MID_PRIV_DEF".to_owned()));
		assert!(!public_defines.contains(&"LEAF_PRIV_PUB_DEF".to_owned()));

		// Properties blocked by transitive private link boundaries
		assert!(!public_defines.contains(&"LEAF_PUB_PRIV_DEF".to_owned()));
		assert!(!public_defines.contains(&"LEAF_PRIV_PRIV_DEF".to_owned()));

		// Shared properties
		assert!(public_defines.contains(&"LEAF_SHARED_DEF".to_owned()));
		assert_eq!(public_defines.len(), 4, "Defines should be deduplicated");
	}
}
