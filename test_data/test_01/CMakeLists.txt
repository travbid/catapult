cmake_minimum_required(VERSION 3.20)

cmake_policy(SET CMP0091 NEW)

project(test_one)

enable_language(ASM_NASM)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)  # Use -std=c++17 instead of -std=gnu++17
# set(CMAKE_CXX_VISIBILITY_PRESET hidden)
# set(CMAKE_VISIBILITY_INLINES_HIDDEN YES)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

add_subdirectory(submodules/blobject)
add_subdirectory(submodules/my_depend)
add_subdirectory(submodules/nasmproj)

add_library(mylib STATIC mylib.cpp)
target_link_libraries(mylib PRIVATE my_depend_lib)

add_library(zstd INTERFACE IMPORTED)
target_link_libraries(zstd
  INTERFACE
    "/Users/travers/Projects/catapult/build_ninja/zstd/compress.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/decompress.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/deprecated.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/entropy.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/errors.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/legacy.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/pool.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/threading.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/xxhash.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/zdict.a"
    "/Users/travers/Projects/catapult/build_ninja/zstd/zstd_common.a"
)

add_executable(myexe main.cpp)
target_include_directories(myexe
  PRIVATE
    "/Users/travers/Library/Caches/catapult/cache/zstd/travbid/main/zstd-1.5.5/lib"
)
target_link_libraries(myexe
  PRIVATE
    mylib
    blobject
    nasmobjs
    zstd
)
target_compile_definitions(myexe PRIVATE MY_DEFINE="abc def")
