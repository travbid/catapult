# Catapult
A package manager + build system for C++. Like Rust's Cargo but for C++.

Inspired by cargo, CMake, conan, buck2, Meson

## Usage
Catapult requires two files in your project: `catapult.toml` and `build.catapult`.
`catapult.toml` is comparable to `Cargo.toml` and contains metadata about the package.
`build.catapult` is like a CMake script, providing a recipe to build the package.

Dependencies can be specified in `catapult.toml`:
```toml
[dependencies]
zstd = { version = "1.5.5", registry = "https://catapult.trav.bid:6400", channel = "travbid/main"}
```

`build.catapult` files are written in [starlark](https://bazel.build/rules/language), a subset of Python.
They should look vaguely familiar if you know CMake:
```python
mylib = add_library(
    name = 'mylib',
    sources = ['mylib.cpp'],
    include_dirs_private = zstd.include_dirs,
    include_dirs_public = ['.'],
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
catapult --source-dir . --build-dir build --generator Ninja
ninja -C build
```



