; framebuffer_fixture (bead 02r): descriptor-backed FRAMEBUFFER region.
;
; This is capture_fixture's manifest shape with one important difference:
; the FRAMEBUFFER region starts with the host-facing descriptor
; {width,height,stride,pixel_format} and then exactly stride*height bytes
; of known XRGB8888 pixels. It exists to exercise GetFramebuffer and
; descriptor-aware CaptureSpec.framebuffer parsing without weakening the
; raw capture_fixture coverage.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT     0x3F8

%define PORT_INIT_LO    0xD374
%define PORT_INIT_HI    0xD378
%define PORT_INIT_GO    0xD37C

%define CHANNEL_GPA     0x400000
%define CHANNEL_PAGES   512

%define MANIFEST_OFF    0x1000
%define MANIFEST_MAGIC  0x46445444
%define OFF_ENTRY0      0x20
%define OFF_EXTENT0     0x1820

%define FB_GPA          0x600000
%define FB_WIDTH        8
%define FB_HEIGHT       4
%define FB_STRIDE       32
%define FB_FORMAT       1
%define FB_PIXEL_BYTES  128
%define FB_BYTES        144
%define FB_QWORDS       16
%define FB_QWORD_BASE   0xFD00000000000000

%define REGION_FLAG_FRAMEBUFFER 1
%define DEFAULT_LAYOUT_VERSION  1
%define REGION_NAME_LEN 11

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .fail_f
    cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
    jne     .fail_f
    mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
    cmp     rax, FB_GPA + FB_BYTES
    jb      .fail_f

    mov     ecx, DEFAULT_LAYOUT_VERSION
    mov     r9d, [rsi + BOOTINFO_OFF_CMDLINE_LEN]
    lea     r8, [rsi + BOOTINFO_OFF_CMDLINE]
    xor     rax, rax
    xor     r10d, r10d
.parse:
    test    r9d, r9d
    jz      .parse_done
    movzx   edx, byte [r8]
    sub     edx, '0'
    cmp     edx, 9
    ja      .parse_done
    imul    rax, rax, 10
    add     rax, rdx
    mov     r10d, 1
    inc     r8
    dec     r9d
    jmp     .parse
.parse_done:
    test    r10d, r10d
    jz      .have_version
    test    eax, eax
    jz      .have_version
    mov     ecx, eax
.have_version:
    mov     [layout_version], ecx

    ; Descriptor: u32 width, height, stride, pixel_format.
    mov     rdi, FB_GPA
    mov     dword [rdi], FB_WIDTH
    mov     dword [rdi + 4], FB_HEIGHT
    mov     dword [rdi + 8], FB_STRIDE
    mov     dword [rdi + 12], FB_FORMAT

    ; Pixel qword j = FB_QWORD_BASE + j.
    add     rdi, 16
    mov     rcx, FB_QWORDS
    mov     rax, FB_QWORD_BASE
.fill:
    mov     [rdi], rax
    add     rdi, 8
    add     rax, 1
    loop    .fill

    ; Channel header, identical to capture_fixture/device_exercise.
    mov     rbx, CHANNEL_GPA
    mov     rax, 0x5453455547544544
    mov     [rbx], rax
    mov     dword [rbx + 0x08], 1
    mov     dword [rbx + 0x0C], 0
    mov     dword [rbx + 0x10], 0x8000
    mov     dword [rbx + 0x14], 0x4000
    mov     dword [rbx + 0x18], 0xC000
    mov     dword [rbx + 0x1C], 0x4000
    mov     dword [rbx + 0x20], 0x10000
    mov     dword [rbx + 0x24], 0x10000
    mov     dword [rbx + 0x28], 0x20000
    mov     dword [rbx + 0x2C], 0x100000

    ; Manifest: one live framebuffer entry and one contiguous extent.
    lea     rdi, [rbx + MANIFEST_OFF]
    mov     dword [rdi], MANIFEST_MAGIC
    mov     dword [rdi + 4], 0x00400001
    mov     dword [rdi + 16], 1
    mov     dword [rdi + 20], 1

    mov     dword [rdi + OFF_ENTRY0 + 4], 1
    mov     ecx, [layout_version]
    mov     [rdi + OFF_ENTRY0 + 8], ecx
    mov     dword [rdi + OFF_ENTRY0 + 12], REGION_FLAG_FRAMEBUFFER
    mov     qword [rdi + OFF_ENTRY0 + 24], FB_BYTES
    mov     dword [rdi + OFF_ENTRY0 + 36], 1
    lea     rsi, [region_name]
    lea     rdi, [rbx + MANIFEST_OFF + OFF_ENTRY0 + 40]
    mov     rcx, REGION_NAME_LEN
    rep movsb

    lea     rdi, [rbx + MANIFEST_OFF + OFF_EXTENT0]
    mov     qword [rdi], FB_GPA
    mov     qword [rdi + 8], FB_BYTES

    mov     al, 'F'
    call    putc

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

    mov     al, 'X'
    call    putc
    ret

.fail_f:
    mov     al, 'f'
    jmp     .fail_out
.fail_d:
    mov     al, 'd'
.fail_out:
    call    putc
    ret

putc:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret

SECTION .rodata
region_name: db "framebuffer"

SECTION .bss
align 4
layout_version: resd 1
