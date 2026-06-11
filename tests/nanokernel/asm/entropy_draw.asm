; entropy_draw (bead dy8): the M4 ENTR-golden guest. An infinite loop of
; fixed-size entropy draws through the REAL pv-entropy MMIO path: program
; LEN=16 once, then per draw point BUF_GPA at the next ring slot, ring
; the DOORBELL (the device fills guest RAM synchronously from the slot's
; DetEntropy and logs the AUX ENTROPY digest), verify STATUS==OK, bump
; the count. A STATUS fault halts loudly — the harness treats
; GuestHalted as failure for this guest.
;
; Draw ring (host reads at TABLE_GPA; lib.rs mirrors, drift-tested):
;   0x00 u64 count (MONOTONE, never wraps), then 2^15 slots x 16 raw
;   draw bytes; draw i lands at slot (i & RING_MASK), so header + ring
;   end at 0x580008 — bounded regardless of how long the harness runs.
;   The count is bumped AFTER the device wrote the slot (torn-read
;   discipline for a host sampling mid-run).
;
; BATCHED: the guest draws BATCH_DRAWS fills, then HLTs; re-entering
; resumes the outer loop for the next batch. The harness runs one
; segment per batch and stops on GuestHalted — an EXACT exit with zero
; skid — so no PMI landing ever has to single-step across the MMIO
; instructions (iteration-82 empirics: an MMIO-write exit can eat the
; single-step trap and free-run past a poll target; see the
; landing-vs-mmio bead). Fixed pacing between draws keeps batch cost
; icount-deterministic, same idea as pad_echo.

BITS 64

%define TABLE_GPA    0x500000
%define RING_MASK    0x7FFF
%define DRAW_BYTES   16
%define ENT_BASE     0xD0003000
%define REG_BUF_GPA  0x08
%define REG_LEN      0x10
%define REG_DOORBELL 0x14
%define REG_STATUS   0x18
%define STATUS_OK    1
%define PACE_ITERS   16
%define BATCH_DRAWS  256

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     r8, ENT_BASE
    mov     r9, TABLE_GPA
    mov     eax, DRAW_BYTES
    mov     [r8 + REG_LEN], eax       ; constant-size draws, programmed once

.batch:
    mov     r13d, BATCH_DRAWS
.draw:
    ; slot address = TABLE + 8 + (count & RING_MASK) * DRAW_BYTES
    mov     rcx, [r9]
    mov     rdx, rcx
    and     rdx, RING_MASK
    shl     rdx, 4
    lea     rdx, [r9 + 8 + rdx]
    mov     [r8 + REG_BUF_GPA], rdx   ; 8-byte MMIO write
    mov     dword [r8 + REG_DOORBELL], 1
    mov     eax, [r8 + REG_STATUS]
    cmp     eax, STATUS_OK
    jne     .fault
    add     rcx, 1
    mov     [r9], rcx

    ; fixed pacing: PACE_ITERS x 7-instruction busy iterations
    lea     r12, [work_buf]
    xor     ebx, ebx
    mov     r11d, PACE_ITERS
    mov     rax, 0x5EED
.pace:
    imul    rax, rax, 31
    add     rax, 7
    mov     [r12 + rbx*8], rax
    add     ebx, 1
    and     ebx, 511
    sub     r11d, 1
    jnz     .pace
    sub     r13d, 1
    jnz     .draw
    hlt                               ; batch boundary: exact, zero skid
    jmp     .batch

.fault:
    ; Inert marker only (0xDEAD < MAX_FILL, and no doorbell ever rings
    ; again) — what actually trips the harness is the COUNT SHORTFALL:
    ; the batch never completes, so the exact-count assert fails.
    mov     dword [r8 + REG_LEN], 0xDEAD
.fault_spin:
    hlt
    jmp     .fault_spin

SECTION .bss
align 64
work_buf:   resq 512
