; fake_frames (bead r2y): the M5 at_frame/frame_budget acceptance guest —
; a pure fake-frame emitter. Bumps pv-pad FRAME_COUNTER (§6.4) on a FIXED
; busy-loop cadence so every frame-boundary MMIO write lands at a
; deterministic icount and the host's FRAME_MARK table populates evenly.
;
; THE LOAD-BEARING DIFFERENCE from pad_echo: F is initialized by READING
; the device's FRAME_COUNTER at entry, not from zero. FRAME_COUNTER is
; lineage-ABSOLUTE (strictly increasing across snapshot/restore, §6.4);
; a register-tracked F survives an ordinary restore anyway (guest
; registers are restored), but reading the device makes continuity hold
; BY CONSTRUCTION for every composition — including a fresh-booted guest
; against a restored device state — so the 5yo acceptance can assert
; strict increase across the snapshot/restore seam unconditionally.
;
; Serial: a single 'G' after the initial read (boot proof), then silent —
; the FRAME_MARK table is the observable. No pad polling, no RAM table:
; this guest exists to make frames, nothing else (pad_echo is the
; pad-input guest).
;
; Cadence: PACE_ITERS x 7-instruction busy iterations between bumps,
; identical to pad_echo's pace loop (drift-pinned). The exact
; instructions-per-frame is not load-bearing for at_frame scheduling —
; the harness reads boundary icounts from the FRAME_MARK table — but the
; cadence must be FIXED so record and replay produce the identical table.

BITS 64

%define PAD_BASE     0xD0001000
%define REG_FRAME    0x1C
%define SERIAL_PORT  0x3F8
%define PACE_ITERS   64

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     r8, PAD_BASE
    mov     r10d, [r8 + REG_FRAME]   ; continue from the device's absolute F

    mov     al, 'G'                  ; boot proof, exactly once
    mov     dx, SERIAL_PORT
    out     dx, al

.frame:
    add     r10d, 1
    mov     [r8 + REG_FRAME], r10d   ; frame boundary (AUX FRAME_MARK)

    ; fixed pacing: PACE_ITERS x 7-instruction busy iterations (same
    ; body as pad_echo's pace loop; the `and ebx, 511` bounds work_buf
    ; writes if pacing is ever retuned past 512)
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
