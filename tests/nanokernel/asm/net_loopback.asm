; net_loopback (bead fbr): the M5 NET_RX landing guest — drives the
; pv-net loopback (ARCH §6.7, window 0xD000_5000) end to end: publishes
; an RX buffer, TXes one known frame, spins polling RX_LEN until run
; control's loopback path re-lands the frame as a NET_RX delivery, then
; verifies the payload byte-for-byte against what it sent.
;
; Serial progress bytes (harness reads "TRX" as full success):
;
;   'T' TX doorbell rang, TX_STATUS == OK (AUX NET_TX logged host-side)
;   'R' RX delivered: RX_LEN went nonzero and equals the frame length
;   'X' payload verified byte-identical; RX_LEN cleared (guest's job)
;
; On a stage failure the LOWERCASE letter goes out and the guest parks:
; 't' TX fault, 'r' spin budget exhausted or wrong RX_LEN, 'x' payload
; mismatch. The spin is BOUNDED so a harness that never delivers gets a
; loud 'r', not a hang.
;
; Polling only (RX_VECTOR stays 0) — the M5 demo path is the polling
; loopback guest (replay_engine module doc); vectored RX needs an IDT
; guest and is a later bead's. The RX buffer is published BEFORE the TX
; doorbell so delivery can never race publication.
;
; Frame content: FRAME_LEN bytes, byte i = (FRAME_BYTE_BASE + i) & 0xFF
; — the harness recomputes the same sequence (NET_TX digest8 and the
; replayed NET_RX both cover these exact bytes).

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT     0x3F8

; pv-net register window (mirrors crates/dh-devices/src/net.rs; the
; elf_shape drift pin compares every %define against the device truth).
%define NET_BASE        0xD0005000
%define REG_TX_BUF_GPA  0x08
%define REG_TX_LEN      0x10
%define REG_TX_DOORBELL 0x14
%define REG_TX_STATUS   0x18
%define REG_RX_BUF_GPA  0x20
%define REG_RX_CAP      0x28
%define REG_RX_LEN      0x2C
%define STATUS_OK       1

; Fixed buffer GPAs (the czq harness reads these back).
%define TX_GPA          0x200000
%define RX_GPA          0x210000
%define RX_CAP_BYTES    2048
%define FRAME_LEN       64
%define FRAME_BYTE_BASE 0x5A
%define SPIN_BUDGET     65536

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    ; ---- BootInfo sane + enough RAM for both buffers? --------------------
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .fail_t
    cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
    jne     .fail_t
    mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
    cmp     rax, RX_GPA + RX_CAP_BYTES
    jb      .fail_t

    ; ---- the known frame: byte i = (FRAME_BYTE_BASE + i) & 0xFF ----------
    mov     rdi, TX_GPA
    mov     ecx, FRAME_LEN
    mov     al, FRAME_BYTE_BASE
.fill:
    mov     [rdi], al
    inc     rdi
    inc     al
    loop    .fill

    ; ---- publish the RX buffer BEFORE ringing TX --------------------------
    mov     r8, NET_BASE
    mov     rax, RX_GPA
    mov     [r8 + REG_RX_BUF_GPA], rax
    mov     dword [r8 + REG_RX_CAP], RX_CAP_BYTES
    ; RX_VECTOR stays 0 (polling); RX_LEN starts 0 (zeroed RAM-like reset)

    ; ---- TX the frame ------------------------------------------------------
    mov     rax, TX_GPA
    mov     [r8 + REG_TX_BUF_GPA], rax
    mov     dword [r8 + REG_TX_LEN], FRAME_LEN
    mov     dword [r8 + REG_TX_DOORBELL], 1
    mov     eax, [r8 + REG_TX_STATUS]
    cmp     eax, STATUS_OK
    jne     .fail_t
    mov     al, 'T'
    call    putc

    ; ---- bounded spin for the loopback delivery ---------------------------
    mov     rcx, SPIN_BUDGET
.spin:
    mov     eax, [r8 + REG_RX_LEN]
    test    eax, eax
    jnz     .delivered
    loop    .spin
    jmp     .fail_r
.delivered:
    cmp     eax, FRAME_LEN
    jne     .fail_r
    mov     al, 'R'
    call    putc

    ; ---- verify payload, clear RX_LEN (consumer's job) --------------------
    mov     rsi, TX_GPA
    mov     rdi, RX_GPA
    mov     ecx, FRAME_LEN
    repe    cmpsb
    jne     .fail_x
    mov     dword [r8 + REG_RX_LEN], 0

    ; ---- success -----------------------------------------------------------
    mov     al, 'X'
    call    putc
    ret                                  ; crt0 parks in HLT

.fail_t:
    mov     al, 't'
    jmp     .fail_out
.fail_r:
    mov     al, 'r'
    jmp     .fail_out
.fail_x:
    mov     al, 'x'
.fail_out:
    call    putc
    ret

; putc: AL -> debug serial. Clobbers DX only.
putc:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret
