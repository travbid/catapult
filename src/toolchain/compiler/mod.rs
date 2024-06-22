mod clang;
mod emscripten;
mod gcc;
mod nasm;

use std::process;

const CLANG_ID: &str = "clang version ";
const EMSCRIPTEN_ID: &str = "emcc ";
const GCC_ID: &str = "gcc version ";
const NASM_ID: &str = "NASM version ";
const TARGET_PREFIX: &str = "Target: ";

pub trait Assembler {
	fn id(&self) -> String;
	fn version(&self) -> String;

	fn cmd(&self) -> Vec<String>;
	fn out_flag(&self) -> String;
}

pub trait Compiler {
	fn id(&self) -> String;
	fn version(&self) -> String;
	fn target(&self) -> String;

	fn cmd(&self) -> Vec<String>;
	fn out_flag(&self) -> String;
	fn c_std_flag(&self, std: &str) -> Result<String, String>;
	fn cpp_std_flag(&self, std: &str) -> Result<String, String>;
	fn position_independent_code_flag(&self) -> Option<String>;
	fn position_independent_executable_flag(&self) -> Option<String>;
}

pub trait StaticLinker {
	fn cmd(&self) -> Vec<String>;
}

pub trait ExeLinker {
	fn cmd(&self) -> Vec<String>;
	fn position_independent_executable_flag(&self) -> Option<String>;
}

pub(super) fn identify_assembler(cmd: Vec<String>) -> Result<Box<dyn Assembler>, String> {
	log::debug!("identify_assembler() cmd: {}", cmd.join(" "));
	let exe = match cmd.first() {
		Some(x) => x,
		None => return Err("Assembler command is empty".to_owned()),
	};
	let version_output = match process::Command::new(exe).arg("-v").output() {
		Ok(x) => {
			if !x.status.success() {
				return Err(format!("Assembler command returned non-success exit code: \"{} -v\": {}", exe, x.status));
			}
			String::from_utf8_lossy(&x.stdout).into_owned() + &String::from_utf8_lossy(&x.stderr)
		}
		Err(e) => {
			return Err(format!("Error executing assembler command \"{} -v\": {}", exe, e));
		}
	};
	log::debug!("{} -v output: {}", exe, version_output);

	let lines = version_output.lines().collect::<Vec<&str>>();
	let first_line = match lines.first() {
		None => return Err("Assembler command output empty. Could not identify assembler".to_owned()),
		Some(x) => x,
	};

	if first_line.starts_with(NASM_ID) {
		log::info!("assembler: NASM");
		let version = find_version(first_line, NASM_ID);
		log::info!("assembler version: {}", version);

		return Ok(Box::new(nasm::Nasm { cmd, version }));
	}

	Err(format!("Could not identify assembler \"{}\"", exe))
}

pub(super) fn identify_compiler(cmd: Vec<String>) -> Result<Box<dyn Compiler>, String> {
	log::debug!("identify_compiler() cmd: {}", cmd.join(" "));
	let exe = match cmd.first() {
		Some(x) => x,
		None => return Err("Compiler command is empty".to_owned()),
	};
	// The `-v` flag is a shorthand for '--verbose' or '--version --verbose'
	// and outputs to stderr instead of stdout
	let version_output = match process::Command::new(exe).arg("-v").output() {
		Ok(x) => {
			if !x.status.success() {
				return Err(format!("Compiler command returned non-success exit code: \"{} -v\": {}", exe, x.status));
			}
			String::from_utf8_lossy(&x.stderr).into_owned()
		}
		Err(e) => {
			return Err(format!("Error executing compiler command \"{} -v\": {}", exe, e));
		}
	};
	log::debug!("{} -v output: {}", exe, version_output);

	let lines = version_output.lines().collect::<Vec<&str>>();
	let first_line = match lines.first() {
		None => return Err("Compiler command output empty. Could not identify compiler".to_owned()),
		Some(x) => x,
	};

	if let Some(clang) = identify_clang(first_line, &lines, &cmd)? {
		Ok(clang)
	} else if let Some(gcc) = identify_gcc(&lines, &cmd)? {
		Ok(gcc)
	} else if let Some(emcc) = identify_emscripten(first_line, &lines, &cmd)? {
		Ok(emcc)
	} else {
		Err(format!("Could not identify compiler \"{}\"", exe))
	}
}

pub(super) fn identify_linker(cmd: Vec<String>) -> Result<Box<dyn ExeLinker>, String> {
	log::debug!("identify_linker() cmd: {}", cmd.join(" "));
	let exe = match cmd.first() {
		Some(x) => x,
		None => return Err("Linker command is empty".to_owned()),
	};
	// The `-v` flag is a shorthand for '--verbose' or '--version --verbose'
	// and outputs to stderr instead of stdout
	let version_output = match process::Command::new(exe).arg("-v").output() {
		Ok(x) => {
			if !x.status.success() {
				return Err(format!("Linker command returned non-success exit code: \"{} -v\": {}", exe, x.status));
			}
			String::from_utf8_lossy(&x.stderr).into_owned()
		}
		Err(e) => {
			return Err(format!("Error executing linker command \"{} -v\": {}", exe, e));
		}
	};
	log::debug!("{} -v output: {}", exe, version_output);

	let lines = version_output.lines().collect::<Vec<&str>>();
	let first_line = match lines.first() {
		None => return Err("Linker command output empty. Could not identify linker".to_owned()),
		Some(x) => x,
	};

	if let Some(clang) = identify_clang(first_line, &lines, &cmd)? {
		Ok(clang)
	} else if let Some(gcc) = identify_gcc(&lines, &cmd)? {
		Ok(gcc)
	} else if let Some(emcc) = identify_emscripten(first_line, &lines, &cmd)? {
		Ok(emcc)
	} else {
		Err(format!("Could not identify linker \"{}\"", exe))
	}
}

fn identify_clang(first_line: &str, lines: &[&str], cmd: &[String]) -> Result<Option<Box<clang::Clang>>, String> {
	if !first_line.starts_with(CLANG_ID) && !first_line.contains(&(String::from(" ") + CLANG_ID)) {
		return Ok(None);
	}
	log::info!("compiler: clang");
	let version = find_version(first_line, CLANG_ID);
	log::info!("compiler version: {}", version);

	let target = match lines.iter().find(|l| l.starts_with(TARGET_PREFIX)) {
		None => return Err(format!("Could not find \"{}\" in compiler output", TARGET_PREFIX)),
		Some(x) => x[TARGET_PREFIX.len()..].to_owned(),
	};
	log::info!("compiler target: {}", target);

	let target_windows = target.contains("-windows-");
	Ok(Some(Box::new(clang::Clang { cmd: cmd.to_vec(), version, target, target_windows })))
}

fn identify_gcc(lines: &[&str], cmd: &[String]) -> Result<Option<Box<gcc::Gcc>>, String> {
	if let Some(line) = lines.iter().find(|l| l.starts_with(GCC_ID)) {
		log::info!("compiler: gcc");

		let version = find_version(line, GCC_ID);
		log::info!("compiler version: {}", version);

		let target = match lines.iter().find(|l| l.starts_with(TARGET_PREFIX)) {
			None => return Err(format!("Could not find \"{}\" in compiler output", TARGET_PREFIX)),
			Some(x) => x[TARGET_PREFIX.len()..].to_owned(),
		};
		log::info!("compiler target: {}", target);

		Ok(Some(Box::new(gcc::Gcc { cmd: cmd.to_vec(), version, target })))
	} else {
		Ok(None)
	}
}

fn identify_emscripten(
	first_line: &str,
	lines: &[&str],
	cmd: &[String],
) -> Result<Option<Box<emscripten::Emscripten>>, String> {
	if !first_line.starts_with(EMSCRIPTEN_ID) {
		return Ok(None);
	}
	log::info!("compiler: emscripten");

	let close_paren_idx = first_line
		.chars()
		.position(|x| x == ')')
		.map_or(EMSCRIPTEN_ID.len(), |x| x + 1);
	let bgn_idx = close_paren_idx
		+ first_line[close_paren_idx..]
			.chars()
			.position(|x| !x.is_whitespace())
			.unwrap_or(0);
	let version = match first_line[bgn_idx..].find(' ') {
		None => &first_line[bgn_idx..],
		Some(offset) => &first_line[bgn_idx..bgn_idx + offset],
	}
	.to_owned();
	log::info!("compiler version: {}", version);

	let target = match lines.iter().find(|l| l.starts_with(TARGET_PREFIX)) {
		None => return Err(format!("Could not find \"{}\" in compiler output", TARGET_PREFIX)),
		Some(x) => x[TARGET_PREFIX.len()..].to_owned(),
	};
	log::info!("compiler target: {}", target);

	Ok(Some(Box::new(emscripten::Emscripten { cmd: cmd.to_vec(), version, target })))
}

fn find_version(line: &str, ver_str: &str) -> String {
	let bgn_idx = line.find(ver_str).unwrap() + ver_str.len();
	let version = match line[bgn_idx..].find(' ') {
		None => &line[bgn_idx..],
		Some(offset) => &line[bgn_idx..bgn_idx + offset],
	};
	version.to_owned()
}

// # Expected outputs

// ## clang on Ubuntu
// Ubuntu clang version 17.0.0 (++20230911073219+0176e8729ea4-1~exp1~20230911073329.40)
// Target: x86_64-pc-linux-gnu
// Thread model: posix
// InstalledDir: /usr/bin
// Found candidate GCC installation: /usr/bin/../lib/gcc/x86_64-linux-gnu/11
// Found candidate GCC installation: /usr/bin/../lib/gcc/x86_64-linux-gnu/12
// Selected GCC installation: /usr/bin/../lib/gcc/x86_64-linux-gnu/12
// Candidate multilib: .;@m64
// Selected multilib: .;@m64
// Found CUDA installation: /usr/local/cuda, version

// ## clang on Windows
// clang version 16.0.1
// Target: x86_64-pc-windows-msvc
// Thread model: posix
// InstalledDir: C:\Program Files\LLVM\bin

// ## gcc on Ubuntu
// Using built-in specs.
// COLLECT_GCC=g++
// COLLECT_LTO_WRAPPER=/usr/lib/gcc/x86_64-linux-gnu/11/lto-wrapper
// OFFLOAD_TARGET_NAMES=nvptx-none:amdgcn-amdhsa
// OFFLOAD_TARGET_DEFAULT=1
// Target: x86_64-linux-gnu
// Configured with: ../src/configure -v --with-pkgversion='Ubuntu 11.4.0-1ubuntu1~22.04' --with-bugurl=file:///usr/share/doc/gcc-11/README.Bugs --enable-languages=c,ada,c++,go,brig,d,fortran,objc,obj-c++,m2 --prefix=/usr --with-gcc-major-version-only --program-suffix=-11 --program-prefix=x86_64-linux-gnu- --enable-shared --enable-linker-build-id --libexecdir=/usr/lib --without-included-gettext --enable-threads=posix --libdir=/usr/lib --enable-nls --enable-bootstrap --enable-clocale=gnu --enable-libstdcxx-debug --enable-libstdcxx-time=yes --with-default-libstdcxx-abi=new --enable-gnu-unique-object --disable-vtable-verify --enable-plugin --enable-default-pie --with-system-zlib --enable-libphobos-checking=release --with-target-system-zlib=auto --enable-objc-gc=auto --enable-multiarch --disable-werror --enable-cet --with-arch-32=i686 --with-abi=m64 --with-multilib-list=m32,m64,mx32 --enable-multilib --with-tune=generic --enable-offload-targets=nvptx-none=/build/gcc-11-XeT9lY/gcc-11-11.4.0/debian/tmp-nvptx/usr,amdgcn-amdhsa=/build/gcc-11-XeT9lY/gcc-11-11.4.0/debian/tmp-gcn/usr --without-cuda-driver --enable-checking=release --build=x86_64-linux-gnu --host=x86_64-linux-gnu --target=x86_64-linux-gnu --with-build-config=bootstrap-lto-lean --enable-link-serialization=2
// Thread model: posix
// Supported LTO compression algorithms: zlib zstd
// gcc version 11.4.0 (Ubuntu 11.4.0-1ubuntu1~22.04)
