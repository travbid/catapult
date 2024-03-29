use std::env;

use catapult::{target::Target, toolchain::Toolchain};

#[test]
fn test_01() {
	assert!(env::set_current_dir("test_data/test_01").is_ok());

	let cwd = env::current_dir().unwrap().canonicalize().unwrap();

	let toolchain = Toolchain::default();
	let (project, global_options) = catapult::parse_project(&toolchain).expect("Could not parse project");
	assert_eq!(project.dependencies.len(), 2);

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
	assert_eq!(my_depend.static_libraries.len(), 1);

	let lib = my_depend.static_libraries.first().unwrap();
	assert_eq!(lib.name, "my_depend_lib");
	assert_eq!(lib.sources.cpp.len(), 1);
	assert_eq!(lib.sources.cpp[0].full, cwd.join("submodules").join("my_depend").join("my_depend.cpp"));

	assert_eq!(project.info.name, "test_one");
	let test_one = project;
	println!("test_one: {:?}", *test_one);
	assert_eq!(test_one.executables.len(), 1);
	assert_eq!(test_one.static_libraries.len(), 1);

	let exe = test_one.executables.first().unwrap();
	assert_eq!(exe.name, "myexe");
	assert_eq!(exe.sources.cpp.len(), 1);
	assert_eq!(exe.sources.cpp[0].full, cwd.join("main.cpp"));
	assert_eq!(exe.links.len(), 3);
	assert_eq!(exe.links[0].name(), "mylib");
	assert_eq!(exe.links[1].name(), "my_depend_lib");
	assert_eq!(exe.links[2].name(), "zstd");

	let lib = test_one.static_libraries.first().unwrap();
	assert_eq!(lib.name, "mylib");
	assert_eq!(lib.sources.cpp.len(), 1);
	assert_eq!(lib.sources.cpp[0].full, cwd.join("mylib.cpp"));
}
