#pragma once

#ifdef __cplusplus
extern "C" {
#endif

int asm_add_two_numbers(int a, int b) asm("asm_add_two_numbers");

#ifdef __cplusplus
}
#endif
