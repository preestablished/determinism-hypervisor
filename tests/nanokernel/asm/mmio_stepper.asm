; mmio_stepper (bead 4a3): the landing-vs-MMIO probe guest. A long-mode
; loop whose body is exactly the MMIO cluster the entropy_draw guest
; (and any doorbell-style device guest) executes — an immediate dword
; MMIO write, an 8-byte register MMIO write, an MMIO read — followed by
; nop pacing. The probe lands single-step walks at targets deep inside
; the loop; the walk must cross hundreds of MMIO exits without losing
; the single-step trap (the iteration-82 goal-poll overshoot).
;
; The MMIO window is unbacked hole space — the probe's on_exit acks
; writes and zero-fills reads; no device model is involved, isolating
; the KVM emulation/trap interaction from device side effects.

BITS 64

%define MMIO_BASE   0xD0001000
%define ITERS       400

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     rbx, MMIO_BASE
    mov     ecx, ITERS
.l:
    mov     dword [rbx + 0x14], 1    ; imm dword MMIO write
    mov     [rbx + 0x08], rbx        ; 8-byte reg MMIO write
    mov     eax, [rbx + 0x18]        ; dword MMIO read
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    sub     ecx, 1
    jnz     .l
.park:
    hlt
    jmp     .park
