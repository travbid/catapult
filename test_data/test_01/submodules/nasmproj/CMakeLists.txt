
add_library(nasmobjs OBJECT
    nasmproj.h
    nasmsrc.asm
)

target_include_directories(my_depend_lib
  PUBLIC .
)
