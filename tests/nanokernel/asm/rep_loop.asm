; rep_loop (bead 8g1): the REP-string landing torture guest. An endless
; 6-instruction loop whose heart is a 64-byte REP MOVSB, built so that
; RCX is a mid-REP detector all by itself:
;
;   RCX == 64  ⟺  RIP is AT the REP MOVSB, zero iterations done
;   RCX == 0   ⟺  RIP is anywhere else in the loop
;   0 < RCX < 64  ⟺  a boundary was declared MID-REP — the §3.2
;                    violation the landing test asserts never happens.
;
; Loop body (6 retired instructions per iteration; REP MOVSB retires as
; ONE on completion, §3.1):
;   lea rsi,[src] / lea rdi,[dst] / mov rcx,64 / rep movsb /
;   add r8,1 / jmp .loop
;
; Runs forever; the landing harness abandons the VM after its last
; target (no serial, no HLT, no MMIO — every exit is a harness error).

BITS 64

SECTION .text
global prog_main

prog_main:
.loop:
    lea     rsi, [src_buf]
    lea     rdi, [dst_buf]
    mov     rcx, 64
    rep movsb
    add     r8, 1
    jmp     .loop

SECTION .data
align 64
src_buf: times 64 db 0x5A

SECTION .bss
align 64
dst_buf: resb 64
