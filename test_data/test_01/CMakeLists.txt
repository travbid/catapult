cmake_minimum_required(VERSION 3.20)

cmake_policy(SET CMP0091 NEW)

project(test_one)

set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

add_subdirectory(submodules/blobject)
add_subdirectory(submodules/my_depend)

add_library(mylib STATIC mylib.cpp)
target_link_libraries(mylib PRIVATE my_depend_lib)

add_executable(myexe main.cpp)
target_link_libraries(myexe PRIVATE mylib blobject)
target_compile_definitions(myexe PRIVATE MY_DEFINE="abc def")
