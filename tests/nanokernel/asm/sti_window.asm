; sti_window: interrupt-window test guest (inject.rs live coverage). Runs
; a fixed pad of NOPs with IF=0 (the boot path enters with RFLAGS = 2),
; executes STI — opening the interrupt window after the one-instruction
; STI shadow — then spins. A deferred injection lands at the first
; injectable boundary after the shadow, deterministically.

BITS 64

SECTION .text
global prog_main

prog_main:
    nop
    nop
    nop
    nop
    nop
    sti
.spin:
    jmp     .spin
