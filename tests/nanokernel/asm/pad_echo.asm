; pad_echo (bead 29a): the M5 record/replay guest. Each fake frame it
; bumps FRAME_COUNTER (the frame-boundary MMIO write; the pv-pad device
; logs the AUX FRAME_MARK), polls the PAD0 latch, appends (frame, pad0)
; to the RAM table at TABLE_GPA, and echoes pad0's low byte to the debug
; serial. Frames are paced by a FIXED busy loop so every frame boundary
; lands at a deterministic icount — a scripted pad sequence then produces
; guest-visible state (table + serial stream) the hash chain covers.
;
; Polling only: no IDT, no STI — the pad IRQ_VECTOR stays 0 (the device
; default) and PAD_SET landings change the latch between polls.
;
; Table (host reads at TABLE_GPA; lib.rs mirrors, drift-tested):
;   0x00 u64 count, then 8 bytes per frame: frame u32 LE | pad0 u32 LE.
; The frame loop is the only writer; RAM is zeroed at boot so count
; starts at 0.

BITS 64

%define TABLE_GPA    0x300000
%define PAD_BASE     0xD0001000
%define REG_PAD0     0x08
%define REG_FRAME    0x1C
%define SERIAL_PORT  0x3F8
%define PACE_ITERS   64

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     r8, PAD_BASE
    mov     r9, TABLE_GPA
    xor     r10d, r10d               ; F: frames completed so far

.frame:
    add     r10d, 1
    mov     [r8 + REG_FRAME], r10d   ; frame boundary (AUX FRAME_MARK)
    mov     eax, [r8 + REG_PAD0]     ; poll the latch

    ; append (F, pad0) to the table
    mov     rcx, [r9]
    lea     rdx, [r9 + 8 + rcx*8]
    mov     [rdx], r10d
    mov     [rdx + 4], eax
    add     rcx, 1
    mov     [r9], rcx

    ; echo pad0's low byte to the debug serial
    mov     dx, SERIAL_PORT
    out     dx, al

    ; fixed pacing: PACE_ITERS x 6-instruction busy iterations
    lea     r12, [work_buf]
    xor     ebx, ebx
    mov     r11d, PACE_ITERS
    mov     rax, 0x9AD5
.pace:
    imul    rax, rax, 31
    add     rax, 7
    mov     [r12 + rbx*8], rax
    add     ebx, 1
    and     ebx, 511
    sub     r11d, 1
    jnz     .pace
    jmp     .frame

SECTION .bss
align 64
work_buf:   resq 512
