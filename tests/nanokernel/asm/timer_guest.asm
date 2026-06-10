; timer_guest (bead 583): the M3-accept interrupt guest. Builds a minimal
; 64-bit IDT with recording ISRs for vectors 0x40 and 0x41, then:
;
;   default        STI and spin — the host injects vectors (§3.4) and
;                  reads the delivery table back from guest RAM.
;   cmdline "mask" never STI — the IF=0 deferral variant: any injection
;                  must defer (WindowNeverOpened under a bounded budget).
;   cmdline "arm"  the full M3 arming loop (pv-clock TIMER_DEADLINE MMIO
;                  every 1ms-vns for 10s-vns) — REQUIRES the device-bus
;                  run loop (bead 40q); under today's debug loops an MMIO
;                  access is a loud foreign exit.
;
; Delivery table (host reads it at TABLE_GPA; lib.rs mirrors, drift-
; tested): 0x00 u64 count, 0x08.. one byte per delivery = the vector id,
; in delivery order. ISRs are the only writers.

BITS 64

%include "bootinfo.inc"

%define TABLE_GPA       0x200000
%define CLOCK_BASE      0xD0000000
%define CLOCK_VNS       0x08
%define CLOCK_DEADLINE  0x18
%define ARM_PERIOD_VNS  1000000      ; 1 ms
%define ARM_TOTAL_VNS   10000000000  ; 10 s

SECTION .text
global prog_main
extern BOOT_INFO_PTR

; Interrupt gate for VECTOR -> HANDLER (IDT base in rdi; clobbers rax rbx).
%macro SETGATE 2
    lea     rax, [%2]
    lea     rbx, [rdi + %1 * 16]
    mov     [rbx], ax                ; offset 0..15
    mov     word [rbx + 2], 0x08     ; CS selector
    mov     word [rbx + 4], 0x8E00   ; P=1 DPL=0 interrupt gate, IST=0
    shr     rax, 16
    mov     [rbx + 6], ax            ; offset 16..31
    shr     rax, 16
    mov     [rbx + 8], eax           ; offset 32..63
%endmacro

prog_main:
    ; ---- GDT first: interrupt delivery RELOADS CS from the GDT (the
    ; loader only fills the segment caches; without an in-memory GDT the
    ; descriptor fetch reads zeros and triple-faults). Entries match the
    ; cached selectors: 0x08 code64, 0x10 data.
    lea     rax, [gdt]
    mov     [gdtr + 2], rax
    lgdt    [gdtr]

    ; ---- IDT: vectors 0x40 and 0x41 -> recording ISRs -------------------
    lea     rdi, [idt]
    SETGATE 0x40, isr_40
    SETGATE 0x41, isr_41
    lea     rax, [idt]
    mov     [idtr + 2], rax
    lidt    [idtr]

    ; ---- mode select from the cmdline (first byte: 'm'ask / 'a'rm) ------
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .open_window
    cmp     dword [rsi + BOOTINFO_OFF_CMDLINE_LEN], 0
    je      .open_window
    movzx   eax, byte [rsi + BOOTINFO_OFF_CMDLINE]
    cmp     al, 'm'
    je      .masked
    cmp     al, 'a'
    je      .arm_mode

.open_window:
    sti
.masked:
    ; Deterministic busy work, window per mode above.
    lea     r12, [work_buf]
    xor     rdx, rdx
    mov     rax, 0x1D1E5
.spin:
    imul    rax, rax, 31
    add     rax, 7
    mov     [r12 + rdx*8], rax
    add     rdx, 1
    and     rdx, 511
    jmp     .spin

.arm_mode:
    ; The full M3 arming loop (needs the device-bus run loop, bead 40q):
    ; every iteration arms deadline = current vns + 1ms until 10s elapse.
    sti
    mov     rbx, CLOCK_BASE
.arm_next:
    mov     rax, [rbx + CLOCK_VNS]
    cmp     rax, ARM_TOTAL_VNS
    jae     .arm_done
    add     rax, ARM_PERIOD_VNS
    mov     [rbx + CLOCK_DEADLINE], rax
.wait:
    mov     rcx, [rbx + CLOCK_VNS]
    cmp     rcx, rax
    jb      .wait
    jmp     .arm_next
.arm_done:
    ret                              ; crt0 parks in HLT

; ---- recording ISRs ------------------------------------------------------
%macro RECORD 1
    push    rax
    push    rbx
    mov     rax, TABLE_GPA
    mov     rbx, [rax]               ; count
    mov     byte [rax + 8 + rbx], %1
    inc     qword [rax]
    pop     rbx
    pop     rax
    iretq
%endmacro

isr_40: RECORD 0x40
isr_41: RECORD 0x41

SECTION .data
align 8
gdtr:   dw  3 * 8 - 1
        dq  0                        ; base patched at runtime
align 8
gdt:    dq  0                        ; null
        dq  0x00209A0000000000       ; 0x08: 64-bit code (L=1, P, S, RX)
        dq  0x0000920000000000       ; 0x10: data (P, S, RW)
align 8
idtr:   dw  0x42 * 16 - 1            ; covers vectors 0..0x41
        dq  0                        ; base patched at runtime

SECTION .bss
align 4096
idt:        resb 0x42 * 16
align 64
work_buf:   resq 512
