; Shared entry crt (ARCH §2.3): dh-vmm enters long mode directly at
; e_entry with RSI = &BootInfo. _start stashes the pointer, gives the
; program a stack, and never returns control to "nothing" — after
; prog_main returns the guest parks in a HLT loop (the loader treats HLT
; as the guest's terminal stop, kvm.rs ExitEvent::Hlt).
;
; Programs implement `prog_main` (SysV-ish: no args; read BOOT_INFO_PTR)
; and may rely on a 16 KiB stack and a zeroed .bss.

BITS 64

%include "bootinfo.inc"

SECTION .text.start
global _start
extern prog_main

_start:
    mov     [BOOT_INFO_PTR], rsi
    lea     rsp, [stack_top]
    cld
    call    prog_main
.park:
    hlt
    jmp     .park

SECTION .bss
global BOOT_INFO_PTR
align 8
BOOT_INFO_PTR:  resq 1
align 16
stack_bottom:   resb 16384
stack_top:
