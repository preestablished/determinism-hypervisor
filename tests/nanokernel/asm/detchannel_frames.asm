; detchannel_frames: a public M5 frame-budget regression guest.
;
; It initializes detchannel, then for every frame publishes a ring-W
; FrameMark record and only then writes pv-pad FRAME_COUNTER. This matches
; guest-sdk's normal non-full-ring frame_mark path: no W doorbell is rung
; unless a critical event has to retry on a full ring.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT     0x3F8

%define PAD_BASE        0xD0001000
%define PAD_FRAME       0x1C

; detcall ports (guest-sdk API.md §5)
%define PORT_INIT_LO    0xD374
%define PORT_INIT_HI    0xD378
%define PORT_INIT_GO    0xD37C
; Channel page: same clean-room layout as device_exercise.asm.
%define CHANNEL_GPA     0x400000
%define CHANNEL_PAGES   512
%define RINGW_PROD_OFF  0x280
%define RINGW_DATA_OFF  0x20000
%define RINGW_SIZE      0x100000
%define RINGW_MASK      0x0FFFFF

%define FRAME_RECORD_LEN 24
%define EVENT_FRAME_MARK 13
%define EVENT_PAD        0

; Keep frames close enough for a cheap live-KVM test, but not back-to-back.
%define FRAME_PACE_ITERS 64

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    ; enough RAM for the donated channel page?
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .fail_d
    mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
    cmp     rax, CHANNEL_GPA + 0x200000
    jb      .fail_d

    ; header: magic "DETGUEST" (LE u64), proto_version 1, flags 0
    mov     rbx, CHANNEL_GPA
    mov     rax, 0x5453455547544544      ; "DETGUEST" little-endian
    mov     [rbx], rax
    mov     dword [rbx + 0x08], 1
    mov     dword [rbx + 0x0C], 0
    ; ring_desc[4] {offset u32, size u32}: C, I, A, W
    mov     dword [rbx + 0x10], 0x8000
    mov     dword [rbx + 0x14], 0x4000
    mov     dword [rbx + 0x18], 0xC000
    mov     dword [rbx + 0x1C], 0x4000
    mov     dword [rbx + 0x20], 0x10000
    mov     dword [rbx + 0x24], 0x10000
    mov     dword [rbx + 0x28], 0x20000
    mov     dword [rbx + 0x2C], 0x100000

    ; CHANNEL_INIT detcalls.
    mov     eax, CHANNEL_GPA & 0xFFFFFFFF
    mov     dx, PORT_INIT_LO
    out     dx, eax
    xor     eax, eax
    mov     dx, PORT_INIT_HI
    out     dx, eax
    mov     eax, CHANNEL_PAGES
    mov     dx, PORT_INIT_GO
    out     dx, eax
    in      eax, dx
    test    eax, eax
    jnz     .fail_d

    mov     al, 'D'
    call    putc

.frame_loop:
    mov     eax, [frame_index]
    inc     eax
    mov     [frame_index], eax

    mov     rbx, CHANNEL_GPA
    mov     edx, [ring_w_prod]
    and     edx, RINGW_MASK
    cmp     edx, RINGW_SIZE - FRAME_RECORD_LEN
    jbe     .write_frame_record

    ; Records never wrap. If the tail cannot hold a FrameMark, write a Pad
    ; covering the tail, consume a seq, and publish the advanced producer.
    lea     rdi, [rbx + RINGW_DATA_OFF]
    add     rdi, rdx
    mov     ecx, RINGW_SIZE
    sub     ecx, edx
    mov     word [rdi], cx
    mov     byte [rdi + 2], EVENT_PAD
    mov     byte [rdi + 3], 0
    mov     eax, [ring_w_seq]
    mov     [rdi + 4], eax
    inc     eax
    mov     [ring_w_seq], eax
    mov     eax, [ring_w_prod]
    add     eax, ecx
    mov     [ring_w_prod], eax
    mov     [rbx + RINGW_PROD_OFF], eax

.write_frame_record:
    mov     edx, [ring_w_prod]
    and     edx, RINGW_MASK
    lea     rdi, [rbx + RINGW_DATA_OFF]
    add     rdi, rdx
    mov     word [rdi], FRAME_RECORD_LEN
    mov     byte [rdi + 2], EVENT_FRAME_MARK
    mov     byte [rdi + 3], 0
    mov     eax, [ring_w_seq]
    mov     [rdi + 4], eax
    inc     eax
    mov     [ring_w_seq], eax
    mov     qword [rdi + 8], 0           ; vnanos
    mov     eax, [frame_index]
    mov     [rdi + 16], eax              ; frame_index
    mov     dword [rdi + 20], 0          ; payload pad

    mov     eax, [ring_w_prod]
    add     eax, FRAME_RECORD_LEN
    mov     [ring_w_prod], eax
    mov     [rbx + RINGW_PROD_OFF], eax

    mov     rdi, PAD_BASE
    mov     eax, [frame_index]
    mov     [rdi + PAD_FRAME], eax

    mov     ecx, FRAME_PACE_ITERS
.pace:
    sub     ecx, 1
    jnz     .pace
    jmp     .frame_loop

.fail_d:
    mov     al, 'd'
    call    putc
    ret

; putc: AL -> debug serial. Clobbers DX only.
putc:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret

SECTION .bss
align 4
frame_index: resd 1
ring_w_prod: resd 1
ring_w_seq:  resd 1
