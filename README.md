# Catapult
A package manager + build system for C++. Like Rust's Cargo but for C++.

Inspired by cargo, CMake, conan, buck2, Meson

This project is very experimental and still in the process of proving out concepts.

## Usage
Catapult requires two files in your project: `catapult.toml` and `build.catapult`.
`catapult.toml` is comparable to `Cargo.toml`/`package.json` and contains metadata about the package.
`build.catapult` is like a CMake script, providing a recipe to build the package.

Dependencies can be specified in `catapult.toml`:
```toml
[dependencies]
zstd = { version = "1.5.5", registry = "https://catapult.trav.bid:6400", channel = "travbid/main"}
```

`build.catapult` files are written in [starlark](https://bazel.build/rules/language), a subset of Python.
They should look vaguely familiar if you know CMake:
```python
mylib = add_static_library(
    name = 'mylib',
    sources = ['mylib.cpp'],
    include_dirs_private = zstd.include_dirs,
    include_dirs_public = ['.'],  # Paths are relative to the directory the build.catapult file is in
    link_private = [my_depend.my_depend_lib, zstd.zstd],
)

add_executable(
    name = 'myexe',
    sources = ['main.cpp'],
    links = [mylib],
)
```

### Build and install catapult
```bash
cargo install --path .
```

### Build a project with catapult
```bash
cd <path to c++ project>
catapult --source-dir . --build-dir build --generator Ninja --toolchain test_data/toolchain_clang.toml
ninja -C build
```
Supported generators are `Ninja` and `MSVC`.

### Toolchains
Toolchain files are in TOML format and specify compiler/linker paths and flags. Catapult will try to detect some information about the selected tools. This allows cross-compilation to be treated almost identically to same-platform compilation.

The toolchain file is also where _profiles_ are defined. These can specify flags for example release or debug builds or define your own profile. A profile can be selected with Catapult's `--profile` flag.
```
catapult -S . -B build -G Ninja --profile Release
```
The MSVC generator however will generate a solution including all defined profiles.

Special configurations exist for the MSVC generator. See [toolchain_msvc.toml](test_data/toolchain_msvc.toml) for examples.

A future version of Catapult will auto-generate a toolchain file for you. For now, you can use `test_data/toolchain_clang.toml` or `test_data/toolchain_msvc.toml` as a base.

## Advantages over other build systems
Catapult is like a combination of CMake + Conan, combined into a single tool.

I've noticed people have difficulty understanding CMake's target-based workflow and dislike its syntax. Conan can be unintuitive and cumbersome to use. Catapult aims to make the target-based workflow easier to understand by using a modern language and to make adding dependencies almost as easy as it is with Cargo or npm. Catapult targets are immutable once created so everything you need to know about a target can be found at a single place e.g. a call to `add_static_library`.

Bazel / Buck are geared toward building Google/Facebook's gigantic monorepos and don't really solve the problem of adding third-party dependencies. Like Catapult, they also use Starlark as a build language. However, their dependencies are specified as strings. Catapult specifies dependencies as objects, potentially allowing intellisense to suggest targets and earlier erroring with more useful messages.

Catapult has some similarities with Meson. I found Meson to be too opinionated for a build system and didn't like how it managed subprojects.

Many other build systems don't support Windows / Visual Studio.
