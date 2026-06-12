; capture_fixture (bead 4ws): the M6 capture-engine fixture guest — the
; ONLY region-manifest producer until guest-sdk lands in Phase 3. It
; publishes a minimal detchannel region manifest with ONE
; FRAMEBUFFER-flagged region of known content and a cmdline-bumpable
; layout_version, so the capture-neutrality (C5) and layout_version
; FAILED_PRECONDITION (C2) acceptance tests have something to resolve
; against (ARCH §6.10).
;
; Serial progress bytes (harness reads "FDX" as full success):
;
;   'F' framebuffer filled (known pattern) + manifest published
;   'D' detchannel CHANNEL_INIT status 0 (manifest snapshotted at attach)
;   'X' all stages passed
;
; On a stage failure the LOWERCASE letter goes out and the guest parks.
;
; layout_version comes from the BootInfo cmdline's leading ASCII decimal
; digits (e.g. cmdline "2" → layout_version 2). No digits, empty cmdline,
; or "0" → DEFAULT_LAYOUT_VERSION — same parse contract as landing_loop.
;
; The framebuffer is RAW known-pattern bytes — no FbInfo descriptor at
; offset 0. This fixture feeds the C2 by-name/layout_version resolution
; and C5 neutrality paths; a C4 FbInfo-decode path needs a richer guest
; (or it would parse pattern qwords as garbage dimensions).
;
; Channel page layout is CLEAN-ROOM from .agents/docs/guest-sdk/ ONLY,
; identical to device_exercise's header (same ring descs — the canonical
; layout, mirrored in nanokernel::DEVICE_EXERCISE_RING_DESCS). The
; manifest area sits at channel offset 0x1000 (API.md §4.1): header
; {magic "DTDF" u32 | manifest_version u16 = 1 | region_capacity u16 = 64
; | generation u64 (even = quiescent) | region_count u32 | extent_count
; u32}, 64 entries of 96 bytes from 0x1020, extent pool {gpa u64, len
; u64} from 0x2820. Entry: region_id u32 | name_id u32 | layout_version
; u32 | flags u32 | gva u64 | len u64 | extent_off u32 | extent_n u32 |
; name[56] NUL-padded. The guest writes the manifest BEFORE CHANNEL_INIT,
; so generation stays 0 (even) — no seqlock dance needed: the host's
; first reader runs at attach, after everything is in place.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT     0x3F8

; detcall ports (guest-sdk API.md §5)
%define PORT_INIT_LO    0xD374
%define PORT_INIT_HI    0xD378
%define PORT_INIT_GO    0xD37C

; Channel page: 2 MiB-aligned guest RAM the program donates.
%define CHANNEL_GPA     0x400000
%define CHANNEL_PAGES   512

; Manifest area (channel offset 0x1000) and its internals (API.md §4.1).
; All three offsets restate detguest-wire truth (header::OFF_MANIFEST,
; RegionEntry::offset(0), Extent::offset(0)) — drift-pinned in elf_shape.
%define MANIFEST_OFF    0x1000
%define MANIFEST_MAGIC  0x46445444   ; "DTDF" little-endian
%define OFF_ENTRY0      0x20         ; first region entry, area-relative
%define OFF_EXTENT0     0x1820       ; extent pool slot 0, area-relative

; The published FRAMEBUFFER region: 64 KiB directly after the channel.
%define FB_GPA          0x600000
%define FB_BYTES        0x10000
%define FB_QWORDS       8192
%define FB_QWORD_BASE   0xFB00000000000000

%define REGION_FLAG_FRAMEBUFFER 1
%define DEFAULT_LAYOUT_VERSION  1
%define REGION_NAME_LEN 11           ; "framebuffer"

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    ; ---- BootInfo sane + enough RAM for channel page + framebuffer? -----
    mov     rsi, [BOOT_INFO_PTR]
    test    rsi, rsi
    jz      .fail_f
    cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
    jne     .fail_f
    mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
    cmp     rax, FB_GPA + FB_BYTES
    jb      .fail_f

    ; ---- layout_version: parse cmdline digits, else default -------------
    ; A trusted test knob, not a hardened parser: the 64-bit accumulator
    ; is narrowed to u32, so a pathological cmdline can wrap — fixture
    ; cmdlines are author-supplied.
    mov     ecx, DEFAULT_LAYOUT_VERSION
    mov     r9d, [rsi + BOOTINFO_OFF_CMDLINE_LEN]
    lea     r8, [rsi + BOOTINFO_OFF_CMDLINE]
    xor     rax, rax                 ; parsed value
    xor     r10d, r10d               ; any-digit flag
.parse:
    test    r9d, r9d
    jz      .parse_done
    movzx   edx, byte [r8]
    sub     edx, '0'
    cmp     edx, 9
    ja      .parse_done              ; first non-digit ends the number
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
    jz      .have_version            ; "0" keeps the default
    mov     ecx, eax
.have_version:
    mov     [layout_version], ecx

    ; ---- fill the framebuffer with the known pattern --------------------
    ; qword j = FB_QWORD_BASE + j; the host-side interop test and the M6
    ; capture tests recompute the same sequence.
    mov     rdi, FB_GPA
    mov     rcx, FB_QWORDS
    mov     rax, FB_QWORD_BASE
.fill:
    mov     [rdi], rax
    add     rdi, 8
    add     rax, 1
    loop    .fill

    ; ---- channel header (same canonical layout as device_exercise) ------
    mov     rbx, CHANNEL_GPA
    mov     rax, 0x5453455547544544      ; "DETGUEST" little-endian
    mov     [rbx], rax
    mov     dword [rbx + 0x08], 1        ; proto_version
    mov     dword [rbx + 0x0C], 0        ; flags
    mov     dword [rbx + 0x10], 0x8000   ; C offset
    mov     dword [rbx + 0x14], 0x4000   ; C size
    mov     dword [rbx + 0x18], 0xC000   ; I offset
    mov     dword [rbx + 0x1C], 0x4000   ; I size
    mov     dword [rbx + 0x20], 0x10000  ; A offset
    mov     dword [rbx + 0x24], 0x10000  ; A size
    mov     dword [rbx + 0x28], 0x20000  ; W offset
    mov     dword [rbx + 0x2C], 0x100000 ; W size (power of two, see device_exercise)

    ; ---- manifest: header, one FRAMEBUFFER entry, one extent ------------
    ; Guest RAM starts zeroed: generation 0 (even), the other 63 entry
    ; slots, NUL name padding, and all reserved fields need no stores.
    lea     rdi, [rbx + MANIFEST_OFF]
    mov     dword [rdi], MANIFEST_MAGIC
    mov     dword [rdi + 4], 0x00400001  ; manifest_version 1 | region_capacity 64
    mov     dword [rdi + 16], 1          ; region_count
    mov     dword [rdi + 20], 1          ; extent_count

    ; entry slot 0 (region_id 0 and gva 0 stay zeroed)
    mov     dword [rdi + OFF_ENTRY0 + 4], 1            ; name_id
    mov     ecx, [layout_version]
    mov     [rdi + OFF_ENTRY0 + 8], ecx                ; layout_version
    mov     dword [rdi + OFF_ENTRY0 + 12], REGION_FLAG_FRAMEBUFFER
    mov     qword [rdi + OFF_ENTRY0 + 24], FB_BYTES    ; len
    ; extent_off 0 stays zeroed
    mov     dword [rdi + OFF_ENTRY0 + 36], 1           ; extent_n
    ; name: "framebuffer", NUL padding already zeroed
    lea     rsi, [region_name]
    lea     rdi, [rbx + MANIFEST_OFF + OFF_ENTRY0 + 40]
    mov     rcx, REGION_NAME_LEN
    rep movsb

    ; extent pool slot 0
    lea     rdi, [rbx + MANIFEST_OFF + OFF_EXTENT0]
    mov     qword [rdi], FB_GPA
    mov     qword [rdi + 8], FB_BYTES

    mov     al, 'F'
    call    putc

    ; ---- CHANNEL_INIT detcalls (manifest snapshotted at attach) ---------
    mov     eax, CHANNEL_GPA & 0xFFFFFFFF
    mov     dx, PORT_INIT_LO
    out     dx, eax
    xor     eax, eax                     ; high GPA bits
    mov     dx, PORT_INIT_HI
    out     dx, eax
    mov     eax, CHANNEL_PAGES
    mov     dx, PORT_INIT_GO
    out     dx, eax
    in      eax, dx                      ; status: 0 = attached
    test    eax, eax
    jnz     .fail_d
    mov     al, 'D'
    call    putc

    ; ---- success --------------------------------------------------------
    mov     al, 'X'
    call    putc
    ret                                  ; crt0 parks in HLT

.fail_f:
    mov     al, 'f'
    jmp     .fail_out
.fail_d:
    mov     al, 'd'
.fail_out:
    call    putc
    ret

; putc: AL -> debug serial. Clobbers DX only.
putc:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret

SECTION .rodata
region_name: db "framebuffer"

SECTION .bss
align 4
layout_version: resd 1
