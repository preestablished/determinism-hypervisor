; mmio_irq_stepper (bead ife): the step_one_entry-vs-MMIO probe guest.
; It is mmio_stepper's emulated-MMIO cluster plus timer_guest's minimal
; IDT/STI and recording ISRs. The host queues vectors at a boundary
; immediately before the cluster; step_one_entry must deliver the ISR,
; service the MMIO exits, re-arm single-step, and return at the exact
; next retired boundary rather than free-running.

BITS 64

%define TABLE_GPA   0x210000
%define MMIO_BASE   0xD0001000
%define ITERS       400

SECTION .text
global prog_main
extern BOOT_INFO_PTR

; Interrupt gate for VECTOR -> HANDLER (IDT base in rdi; clobbers rax rbx).
%macro SETGATE 2
    lea     rax, [%2]
    lea     rbx, [rdi + %1 * 16]
    mov     [rbx], ax
    mov     word [rbx + 2], 0x08
    mov     word [rbx + 4], 0x8E00
    shr     rax, 16
    mov     [rbx + 6], ax
    shr     rax, 16
    mov     [rbx + 8], eax
%endmacro

prog_main:
    ; Interrupt delivery reloads CS from memory; match timer_guest's GDT.
    lea     rax, [gdt]
    mov     [gdtr + 2], rax
    lgdt    [gdtr]

    lea     rdi, [idt]
    SETGATE 0x40, isr_40
    SETGATE 0x41, isr_41
    lea     rax, [idt]
    mov     [idtr + 2], rax
    lidt    [idtr]
    sti

    mov     rbx, MMIO_BASE
    mov     ecx, ITERS
.l:
    mov     dword [rbx + 0x14], 1
    mov     [rbx + 0x08], rbx
    mov     eax, [rbx + 0x18]
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

; ---- recording ISRs ------------------------------------------------------
%macro RECORD 1
    push    rax
    push    rbx
    mov     rax, TABLE_GPA
    mov     rbx, [rax]
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
        dq  0
align 8
gdt:    dq  0
        dq  0x00209A0000000000
        dq  0x0000920000000000
align 8
idtr:   dw  0x42 * 16 - 1
        dq  0

SECTION .bss
align 4096
idt:        resb 0x42 * 16
