
add_library(my_depend_lib STATIC
    my_depend.cpp
    my_depend.hpp
)
target_include_directories(my_depend_lib PUBLIC .)
