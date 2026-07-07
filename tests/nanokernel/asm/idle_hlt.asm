; idle_hlt: wall-clock backstop probe A guest (see
; .agents/requests/nextsdkevent-run-wallclock-backstop/). The
; epoll-parked-agent shape: STI opens the interrupt window (IF=1 after
; the one-instruction shadow), then an idle HLT with no timer armed and
; no SDK event pending. This VMM never creates an in-kernel irqchip
; (dh-vmm/src/lib.rs forbidden-capability list), so KVM cannot emulate
; HLT in-kernel — the HLT must produce KVM_EXIT_HLT back to userspace,
; which runctl unwinds as StopReason::GuestHalted. If this guest ever
; wedges inside KVM_RUN, the backstop question reopens.

BITS 64

SECTION .text
global prog_main

prog_main:
    sti
.park:
    hlt
    jmp     .park
