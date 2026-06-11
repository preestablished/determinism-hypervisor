; page_dirtier (bead 28i): the dirty-ring chaos guest. Writes one byte
; to each of PAGES consecutive 4 KiB pages starting at START_GPA, then
; parks in HLT. 3072 dirtied pages overwhelm a 1024-entry dirty ring
; 3 times over — the harness services KVM_EXIT_DIRTY_RING_FULL by
; harvesting mid-run, and the R8 acceptance is that NOTHING the guest
; did differs from a 65536-ring run (ring-full exits are host-visible
; only) and NO dirty page is lost (the incremental snapshot refs match).

BITS 64

%define START_GPA  0x200000
%define PAGES      3072
%define PAGE_SIZE  4096

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     rdi, START_GPA
    mov     rcx, PAGES
.l:
    mov     byte [rdi], 0x5A
    add     rdi, PAGE_SIZE
    sub     rcx, 1
    jnz     .l
.park:
    hlt
    jmp     .park
