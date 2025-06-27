#pragma once

#ifdef __cplusplus
extern "C" {
#endif

int asm_add_two_numbers(int a, int b)
#if __APPLE__
    asm("asm_add_two_numbers")  // Compile with -fgnu-keywords on clang
#endif
    ;

// void nasm_always(void);

#ifdef __cplusplus
}
#endif
