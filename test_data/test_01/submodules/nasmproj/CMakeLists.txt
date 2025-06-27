
add_library(nasmobjs
  OBJECT
    # fallback.c
    nasmsrc.asm
)

target_include_directories(nasmobjs
  PUBLIC
    .
)
