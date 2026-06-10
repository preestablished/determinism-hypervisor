; landing_loop: the deterministic long-runner for the M2 landing test and
; the M3 1e9 determinism regression (bead 7yr).
;
; The loop body is EXACTLY 8 instructions per iteration (the harness-facing
; constant LANDING_LOOP_INSTRS_PER_ITER in src/lib.rs) — an LCG accumulator
; rolled and stored through a 64 KiB ring buffer, so memory evolves
; predictably and state hashes are meaningful at any pause boundary.
;
; Iteration count: the BootInfo cmdline's leading ASCII decimal digits
; (e.g. cmdline "12500000"). No digits / empty cmdline -> DEFAULT_ITERS
; (12_500_000 iterations = 100M loop instructions). After the loop, 'L'
; goes out on the debug serial port and crt0 parks in HLT.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT 0x3F8
%define DEFAULT_ITERS 12500000
%define BUF_QWORD_MASK 8191          ; 8192 qwords = 64 KiB ring

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    ; ---- iteration count: parse cmdline digits, else default ----------
    mov     rcx, DEFAULT_ITERS
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .have_count
    cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
    jne     .have_count
    mov     r9d, [rsi + BOOTINFO_OFF_CMDLINE_LEN]
    lea     r8, [rsi + BOOTINFO_OFF_CMDLINE]
    xor     rax, rax                 ; parsed value
    xor     r10d, r10d               ; any-digit flag
.parse:
    test    r9d, r9d
    jz      .parse_done
    movzx   edx, byte [r8]
    sub     edx, '0'
    cmp     edx, 9
    ja      .parse_done              ; first non-digit ends the number
    imul    rax, rax, 10
    add     rax, rdx
    mov     r10d, 1
    inc     r8
    dec     r9d
    jmp     .parse
.parse_done:
    test    r10d, r10d
    jz      .have_count
    test    rax, rax
    jz      .have_count              ; "0" keeps the default, never instant-exit
    mov     rcx, rax
.have_count:

    ; ---- loop setup ----------------------------------------------------
    mov     r10, 6364136223846793005 ; LCG multiplier (Knuth MMIX)
    mov     r11, 1442695040888963407 ; LCG increment
    lea     r12, [ring_buf]
    mov     rax, 0x4448424900000001  ; seed: fixed, version-tagged
    xor     edx, edx                 ; ring index

    ; ---- THE LOOP: exactly 8 instructions per iteration ----------------
align 16
.loop:
    imul    rax, r10                 ; 1
    add     rax, r11                 ; 2
    rol     rax, 13                  ; 3
    mov     [r12 + rdx*8], rax       ; 4 memory touch
    add     rdx, 1                   ; 5
    and     rdx, BUF_QWORD_MASK      ; 6
    sub     rcx, 1                   ; 7
    jnz     .loop                    ; 8

    ; ---- done: one serial byte, then crt0 parks ------------------------
    mov     al, 'L'
    mov     dx, SERIAL_PORT
    out     dx, al
    ret

SECTION .bss
align 64
ring_buf:   resq 8192
