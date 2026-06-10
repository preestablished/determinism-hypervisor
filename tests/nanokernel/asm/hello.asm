; hello: the M0 boot-path acceptance stub (bead ehu) — prints "HELLO\n" on
; the debug serial port and parks in crt0's HLT loop. The dh-cli boot bead
; (1mz) accepts M0 by booting this and reading the serial log.
;
; Note vs the bead title: there is no real-mode→long-mode phase to write —
; ARCH §2.3's ELF boot path enters long mode directly (CR0/CR4/EFER/GDT
; via KVM_SET_SREGS, RIP = e_entry), so the "stub" reduces to the print.

BITS 64

%define SERIAL_PORT 0x3F8

SECTION .rodata
msg:    db  "HELLO", 10
%define MSG_LEN 6

SECTION .text
global prog_main

prog_main:
    lea     rsi, [msg]
    mov     rcx, MSG_LEN
    mov     dx, SERIAL_PORT
.next:
    lodsb
    out     dx, al
    loop    .next
    ret                             ; crt0 parks in HLT
