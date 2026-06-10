; sse_probe (bead ttk): proves the loader's CR4.OSFXSR actually enables
; SSE — a compiled (Rust/C) guest's baseline. Runs SSE2 integer ops
; (movdqa / pxor / paddq), checks the result with GP compares, prints
; 'V' on success or 'v' on a wrong value, then parks. Without OSFXSR
; the first movdqa would #UD and (no IDT) triple-fault — a loud
; Shutdown, not a serial byte.

BITS 64

%define SERIAL_PORT 0x3F8

SECTION .text
global prog_main

prog_main:
    movdqa  xmm0, [vec_a]
    movdqa  xmm1, [vec_b]
    pxor    xmm0, xmm1               ; {a0^b0, a1^b1}
    paddq   xmm0, [vec_c]            ; + {c0, c1}
    movdqa  [result], xmm0

    mov     rax, [result]
    mov     rbx, 0x1111111111111111 ^ 0x2222222222222222
    add     rbx, 7
    cmp     rax, rbx
    jne     .bad
    mov     rax, [result + 8]
    mov     rbx, 0x4444444444444444 ^ 0x8888888888888888
    add     rbx, 9
    cmp     rax, rbx
    jne     .bad
    mov     al, 'V'
    jmp     .out
.bad:
    mov     al, 'v'
.out:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret                              ; crt0 parks in HLT

SECTION .data
align 16
vec_a:   dq 0x1111111111111111, 0x4444444444444444
vec_b:   dq 0x2222222222222222, 0x8888888888888888
vec_c:   dq 7, 9

SECTION .bss
align 16
result:  resq 2
