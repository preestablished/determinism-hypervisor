; mwait_park: wall-clock backstop probe B guest (see
; .agents/requests/nextsdkevent-run-wallclock-backstop/) — the best
; known attempt at a non-HLT zero-retirement block inside KVM_RUN.
; MONITOR/MWAIT on a .bss cache line: this VMM does not expose MONITOR
; in guest CPUID, so the expected behavior is #UD (no IDT in the
; nanokernel → escalates to triple fault → shutdown exit), or — on a
; host where MWAIT executes as NOP — fall-through into a PAUSE spin
; that retires instructions until the icount hard cap trips. Every
; branch RETURNS from KVM_RUN; none blocks in-kernel.

BITS 64

SECTION .text
global prog_main

prog_main:
    lea     rax, [rel wait_line]
    xor     ecx, ecx
    xor     edx, edx
    monitor
    xor     ecx, ecx
    xor     eax, eax
    mwait
.spin:
    pause
    jmp     .spin

SECTION .bss
align 64
wait_line:  resb 64
