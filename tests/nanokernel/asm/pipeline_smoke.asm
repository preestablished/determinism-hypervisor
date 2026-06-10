; pipeline_smoke: proves the build pipeline end to end. Checks the BootInfo
; magic/version (ARCH §2.3), reports one status byte on the debug serial
; port (0x3F8, §6.9), and returns to crt0's HLT park.
;
;   'K'  BootInfo magic + version OK
;   'B'  BootInfo bad/missing
;
; The substantive guests (hello real-mode stub, landing loop, device
; exercise) are their own beads; this program exists so the pipeline has a
; built, shape-tested artifact from day one.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT 0x3F8

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .bad
    cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
    jne     .bad
    cmp     dword [rsi + BOOTINFO_OFF_VERSION], BOOTINFO_VERSION
    jne     .bad
    mov     al, 'K'
    jmp     .out
.bad:
    mov     al, 'B'
.out:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret
