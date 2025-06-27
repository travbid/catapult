
section .text
	global asm_add_two_numbers

asm_add_two_numbers:
%ifdef WIN64
	push    rax
	mov     dword [rsp + 4], edx
	mov     dword [rsp], ecx
	mov     eax, dword [rsp]
	add     eax, dword [rsp + 4]
	pop     rcx
%else
	push    rbp
	mov     rbp, rsp
	mov     dword [rbp - 4], edi
	mov     dword [rbp - 8], esi
	mov     eax, dword [rbp - 4]
	add     eax, dword [rbp - 8]
	pop     rbp
%endif
	ret
