; counting (bead d34): the known 1,000-instruction sequence for the
; counting_semantics test (IMPLEMENTATION-PLAN M2; ARCH §3.1 empirics).
;
; The counted REGION is every instruction strictly between the two
; marker OUTs ('S' ... 'E' on the debug serial port) and is EXACTLY
; 1,000 instructions BY CONSTRUCTION: every region instruction is
; emitted through the `I` macro, which increments an assembly-time
; counter; the %if at the end fails the BUILD if the total is not
; 1,000. The one runtime loop is accounted explicitly next to its body.
;
; §3.1 retirement empirics at the markers (MEASURED on the kvm-intel
; class; see nanokernel::COUNTING_DELTA_AT_OUT_EXITS): VM-exiting
; instructions retire ZERO guest instructions — they exit before
; retirement and KVM completes them host-side (RIP skip), invisible to
; the exclude_host=1 counter. The S->E exit-moment window is therefore
; region (1,000) minus its three exiting instructions = 997.
;
; Region composition (the §3.1 cases the M2 test attributes):
;   REP MOVSB, RCX=256  — retires as ONE instruction on completion
;   CPUID (leaf 0)      — in-kernel emulated; retires ZERO (measured)
;   MMIO read           — pv-clock VNS (0xD000_0000+0x08); exits,
;                         retires ZERO (measured)
;   MMIO write          — debug-serial THR mirror (0xD000_6000+0x08),
;                         byte 'M'; exits, retires ZERO (measured)
;   taken jmp/jcc, a not-taken jcc, a 4-iteration dec/jnz loop, and
;   single-uop ALU filler for the balance.

BITS 64

%define SERIAL_PORT 0x3F8
%define MMIO_BASE   0xD0000000
%define CLOCK_VNS   0x08
%define SERIAL_THR  0x6008
%define REP_BYTES   256
%define LOOP_ITERS  4

; Region instruction emitter: one instruction, one count.
%assign ICOUNT 0
%macro I 1+
    %1
    %assign ICOUNT ICOUNT+1
%endmacro

; VM-exiting region instruction: counted AND tallied separately so the
; "3 exiting instructions" in nanokernel::COUNTING_EXIT_INSTRS_IN_REGION
; is build-enforced, not hand-maintained.
%assign EXITCOUNT 0
%macro XI 1+
    I %1
    %assign EXITCOUNT EXITCOUNT+1
%endmacro

SECTION .text
global prog_main

prog_main:
    ; ---- 'S' marker (NOT counted; the region starts after the OUT) ----
    mov     dx, SERIAL_PORT
    mov     al, 'S'
    out     dx, al

    ; ======================= counted region =======================
    ; REP MOVSB: one instruction at full completion (§3.1 REP rule).
    I lea     rsi, [src_buf]
    I lea     rdi, [dst_buf]
    I mov     rcx, REP_BYTES
    I rep movsb

    ; CPUID: clobbers eax/ebx/ecx/edx (dx is re-established below).
    I xor     eax, eax
    XI cpuid

    ; MMIO: a read that exits (pv-clock VNS) and a write that exits
    ; (debug-serial THR mirror — the byte lands in the serial log).
    I mov     rbx, MMIO_BASE
    XI mov     rax, [rbx + CLOCK_VNS]
    XI mov     dword [rbx + SERIAL_THR], 'M'

    ; Branches: taken unconditional, taken conditional, not-taken
    ; conditional — each retires exactly once.
    I xor     rax, rax
    I jmp     .fwd
.fwd:
    I cmp     rax, 0
    I je      .taken
.taken:
    I cmp     rax, 0
    I jne     .never                 ; never taken; falls through

    ; Runtime loop: LOOP_ITERS x (dec + jnz) EXECUTED instructions from
    ; 2 emitted ones — accounted explicitly, not via the I macro.
    I mov     rcx, LOOP_ITERS
.lp:
    dec     rcx
    jnz     .lp
%assign ICOUNT ICOUNT + 2 * LOOP_ITERS

    ; Re-establish dx (CPUID clobbered it), stage the 'E' marker byte —
    ; both inside the region (they sit between the marker OUTs), so they
    ; RETIRE INSIDE the measured window; only the E OUT itself is out.
    I mov     dx, SERIAL_PORT
    I mov     al, 'E'

    ; Single-uop filler for the exact balance.
%assign PAD 1000 - ICOUNT
%rep PAD
    I add     r8, 1
%endrep
    ; ===================== end counted region =====================

%if ICOUNT != 1000
%error counting region must be exactly 1000 instructions
%endif
%if EXITCOUNT != 3
%error region must contain exactly 3 VM-exiting instructions
%endif

    out     dx, al                   ; 'E' marker (NOT counted)
.never:
    ; .never doubles as the never-taken jne target and the fall-through:
    ; nothing can ever sit between the E OUT and this ret.
    ret                              ; crt0 parks in HLT

SECTION .data
align 16
src_buf:
%assign B 0
%rep REP_BYTES
    db (B * 7 + 3) & 0xFF
%assign B B+1
%endrep

SECTION .bss
align 16
dst_buf:    resb REP_BYTES
