use std::{
	collections::BTreeMap, //
	env,
};

use catapult::{
	link_type::LinkPtr, //
	target::Target,
	toolchain::Toolchain,
};

#[test]
fn test_01() {
	assert!(env::set_current_dir("test_data/test_01").is_ok());

	let cwd = env::current_dir().unwrap().canonicalize().unwrap();

	let toolchain = Toolchain::default();
	let (project, global_options) =
		catapult::parse_project(&toolchain, BTreeMap::new()).expect("Could not parse project");
	assert_eq!(project.dependencies.len(), 4);

	assert_eq!(global_options.c_standard, Some("17".to_owned()));
	assert_eq!(global_options.cpp_standard, Some("17".to_owned()));
	assert_eq!(global_options.position_independent_code, Some(true));

	let my_depends = project
		.dependencies
		.iter()
		.filter(|x| x.info.name == "my_depend")
		.collect::<Vec<_>>();
	assert_eq!(my_depends.len(), 1);
	let my_depend = my_depends.first().unwrap();
	assert_eq!(my_depend.executables.len(), 1);

	let my_depend_static_libs = my_depend
		.link_targets
		.iter()
		.filter_map(|x| match x {
			LinkPtr::Static(lib) => Some(lib.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();
	assert_eq!(my_depend_static_libs.len(), 1);

	let static_lib = my_depend_static_libs.first().unwrap();
	assert_eq!(static_lib.name, "my_depend_lib");
	assert_eq!(static_lib.sources.cpp.len(), 1);
	assert_eq!(static_lib.sources.cpp[0].full, cwd.join("submodules").join("my_depend").join("my_depend.cpp"));

	let blobjects = project
		.dependencies
		.iter()
		.filter(|x| x.info.name == "blobject")
		.collect::<Vec<_>>();
	assert_eq!(blobjects.len(), 1);

	let blobject = blobjects.first().unwrap();
	let blobject_obj_libs = blobject
		.link_targets
		.iter()
		.filter_map(|x| match x {
			LinkPtr::Object(lib) => Some(lib.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();
	assert_eq!(blobject_obj_libs.len(), 1);

	let obj_lib = blobject_obj_libs.first().unwrap();
	assert_eq!(obj_lib.name, "blobject");
	assert_eq!(obj_lib.sources.c.len(), 1);
	assert_eq!(obj_lib.sources.c[0].full, cwd.join("submodules").join("blobject").join("blobject2.c"));
	assert_eq!(obj_lib.sources.cpp.len(), 1);
	assert_eq!(obj_lib.sources.cpp[0].full, cwd.join("submodules").join("blobject").join("blobject1.cpp"));

	assert_eq!(project.info.name, "test_one");

	let test_one = project;
	println!("test_one: {:?}", *test_one);
	assert_eq!(test_one.executables.len(), 1);
	assert_eq!(
		test_one
			.link_targets
			.iter()
			.filter_map(|x| match x {
				LinkPtr::Static(lib) => Some(lib.clone()),
				_ => None,
			})
			.count(),
		1
	);
	assert_eq!(
		test_one
			.link_targets
			.iter()
			.filter_map(|x| match x {
				LinkPtr::Shared(lib) => Some(lib.clone()),
				_ => None,
			})
			.count(),
		1
	);

	let exe = test_one.executables.first().unwrap();
	assert_eq!(exe.name, "myexe");
	assert_eq!(exe.sources.cpp.len(), 1);
	assert_eq!(exe.sources.cpp[0].full, cwd.join("main.cpp"));
	assert_eq!(exe.links.len(), 5);
	assert_eq!(exe.links[0].name(), "mylib");
	assert_eq!(exe.links[1].name(), "my_depend_lib");
	assert_eq!(exe.links[2].name(), "blobject");
	assert_eq!(exe.links[3].name(), "nasmobjs");
	assert_eq!(exe.links[4].name(), "zstd");

	let test_one_shared_libs = test_one
		.link_targets
		.iter()
		.filter_map(|x| match x {
			LinkPtr::Shared(lib) => Some(lib.clone()),
			_ => None,
		})
		.collect::<Vec<_>>();
	assert_eq!(test_one_shared_libs.len(), 1);

	let lib = test_one_shared_libs.first().unwrap();
	assert_eq!(lib.name, "mylib");
	assert_eq!(lib.sources.cpp.len(), 1);
	assert_eq!(lib.sources.cpp[0].full, cwd.join("mylib.cpp"));
}
