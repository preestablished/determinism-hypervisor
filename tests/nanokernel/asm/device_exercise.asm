; device_exercise: the M1 acceptance guest (bead 7ys). Touches every pv
; device and the detchannel, reporting one serial progress byte per stage:
;
;   'C' pv-clock   VNS + ICOUNT read, icount non-decreasing across exits
;   'E' pv-entropy 32 bytes drawn, status OK, not all-zero
;   'P' pv-pad     latch read (value is host-injected; presence test)
;   'B' pv-blk     write sector 0, read back, bytes equal
;   'D' detchannel CHANNEL_INIT (status 0) + one Beacon on ring W + doorbell
;   'X' all stages passed
;
; On a stage failure the LOWERCASE letter goes out and the guest parks
; immediately — the harness reads the serial log "CEPBDX" as full success.
;
; Device register maps are this repo's ARCH §6.2-§6.5/§6.9 (mirrored from
; crates/dh-devices constants). The channel page layout is CLEAN-ROOM from
; .agents/docs/guest-sdk/ ONLY: header at 0x000 (magic "DETGUEST" LE u64,
; proto_version u32 = 1, flags, ring_desc[4] {offset u32, size u32} at
; 0x010), ringW_prod at 0x280, ring data C/I/A/W at 0x8000/0xC000/0x10000/
; 0x20000, record framing per API.md §3.0 (len u16 | kind u8 | flags u8 |
; seq u32 | vnanos u64 | payload, 8-byte aligned), Beacon = kind 5,
; payload {beacon_id u32, _pad u32}.

BITS 64

%include "bootinfo.inc"

%define SERIAL_PORT     0x3F8

%define CLOCK_BASE      0xD0000000
%define CLOCK_VNS       0x08
%define CLOCK_ICOUNT    0x10

%define PAD_BASE        0xD0001000
%define PAD_PAD0        0x08

%define ENT_BASE        0xD0002000
%define ENT_BUF_GPA     0x08
%define ENT_LEN         0x10
%define ENT_DOORBELL    0x14
%define ENT_STATUS      0x18
%define ENT_STATUS_OK   1

%define BLK_BASE        0xD0004000
%define BLK_SECTOR      0x08
%define BLK_BUF_GPA     0x10
%define BLK_COUNT       0x18
%define BLK_CMD         0x1C
%define BLK_STATUS      0x20
%define BLK_CMD_READ    1
%define BLK_CMD_WRITE   2
%define BLK_STATUS_OK   0

; detcall ports (guest-sdk API.md §5)
%define PORT_INIT_LO    0xD374
%define PORT_INIT_HI    0xD378
%define PORT_INIT_GO    0xD37C
%define PORT_DOORBELL   0xD380

; Channel page: 2 MiB-aligned guest RAM the program donates (clean-room
; layout, see module header). Requires mem_size >= CHANNEL_GPA + 2 MiB.
%define CHANNEL_GPA     0x400000
%define CHANNEL_PAGES   512
%define RINGW_PROD_OFF  0x280
%define RINGW_DATA_OFF  0x20000

SECTION .text
global prog_main
extern BOOT_INFO_PTR

prog_main:
    ; ---- 'C' pv-clock ---------------------------------------------------
    mov     rbx, CLOCK_BASE
    mov     rax, [rbx + CLOCK_VNS]       ; VNS readable
    mov     r8, [rbx + CLOCK_ICOUNT]
    mov     r9, [rbx + CLOCK_ICOUNT]
    cmp     r9, r8
    jb      .fail_c                      ; icount regressed across exits
    mov     [vns_sample], rax
    mov     al, 'C'
    call    putc

    ; ---- 'E' pv-entropy -------------------------------------------------
    mov     rbx, ENT_BASE
    lea     rax, [ent_buf]
    mov     [rbx + ENT_BUF_GPA], rax
    mov     dword [rbx + ENT_LEN], 32
    mov     dword [rbx + ENT_DOORBELL], 1
    mov     eax, [rbx + ENT_STATUS]
    cmp     eax, ENT_STATUS_OK
    jne     .fail_e
    ; not all-zero: OR the four qwords
    mov     rax, [ent_buf]
    or      rax, [ent_buf + 8]
    or      rax, [ent_buf + 16]
    or      rax, [ent_buf + 24]
    test    rax, rax
    jz      .fail_e
    mov     al, 'E'
    call    putc

    ; ---- 'P' pv-pad -----------------------------------------------------
    mov     rbx, PAD_BASE
    mov     eax, [rbx + PAD_PAD0]        ; latch readable (any value)
    mov     al, 'P'
    call    putc

    ; ---- 'B' pv-blk -----------------------------------------------------
    ; pattern the write buffer (64 qwords = 512 bytes)
    lea     rdi, [blk_wbuf]
    mov     rcx, 64
    mov     rax, 0xB10CB10CB10CB10C
.fill:
    mov     [rdi], rax
    add     rdi, 8
    add     rax, 1
    loop    .fill

    mov     rbx, BLK_BASE
    xor     eax, eax
    mov     [rbx + BLK_SECTOR], rax
    lea     rax, [blk_wbuf]
    mov     [rbx + BLK_BUF_GPA], rax
    mov     dword [rbx + BLK_COUNT], 1
    mov     dword [rbx + BLK_CMD], BLK_CMD_WRITE
    mov     eax, [rbx + BLK_STATUS]
    cmp     eax, BLK_STATUS_OK
    jne     .fail_b
    lea     rax, [blk_rbuf]
    mov     [rbx + BLK_BUF_GPA], rax
    mov     dword [rbx + BLK_CMD], BLK_CMD_READ
    mov     eax, [rbx + BLK_STATUS]
    cmp     eax, BLK_STATUS_OK
    jne     .fail_b
    ; compare 512 bytes
    lea     rsi, [blk_wbuf]
    lea     rdi, [blk_rbuf]
    mov     rcx, 64
.cmp:
    mov     rax, [rsi]
    cmp     rax, [rdi]
    jne     .fail_b
    add     rsi, 8
    add     rdi, 8
    loop    .cmp
    mov     al, 'B'
    call    putc

    ; ---- 'D' detchannel -------------------------------------------------
    ; enough RAM for the donated page?
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
    ; ring_desc[4] {offset u32, size u32}: C, I, A, W (canonical layout)
    mov     dword [rbx + 0x10], 0x8000   ; C offset
    mov     dword [rbx + 0x14], 0x4000   ; C size
    mov     dword [rbx + 0x18], 0xC000   ; I offset
    mov     dword [rbx + 0x1C], 0x4000   ; I size
    mov     dword [rbx + 0x20], 0x10000  ; A offset
    mov     dword [rbx + 0x24], 0x10000  ; A size
    mov     dword [rbx + 0x28], 0x20000  ; W offset
    ; W size: the guest-sdk ARCHITECTURE layout table PRINTED 0x1E0000, but
    ; the same doc's normative rule says ring sizes are POWERS OF TWO
    ; (indices masked by size-1) — 0x1E0000 is not. The largest power of
    ; two fitting 0x20000..0x200000 is 1 MiB; attach rejects anything else
    ; (doc contradiction tracked in beads).
    mov     dword [rbx + 0x2C], 0x100000 ; W size
    ; indices and counters: guest RAM starts zeroed; nothing to do.

    ; CHANNEL_INIT detcalls
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

    ; one Beacon record (kind 5) at ring W offset 0:
    ;   len 24 | kind 5 | flags 0 | seq 0 | vnanos (sampled) | beacon_id
    lea     rdi, [rbx + RINGW_DATA_OFF]
    mov     word [rdi], 24               ; len (16 hdr + 8 payload)
    mov     byte [rdi + 2], 5            ; kind = Beacon
    mov     byte [rdi + 3], 0            ; flags
    mov     dword [rdi + 4], 0           ; seq 0
    mov     rax, [vns_sample]
    mov     [rdi + 8], rax               ; vnanos
    mov     dword [rdi + 16], 0xB33F     ; beacon_id
    mov     dword [rdi + 20], 0          ; payload pad
    ; publish producer index (single vCPU: the OUT below orders it)
    mov     dword [rbx + RINGW_PROD_OFF], 24

    ; doorbell: drain ring W (mask bit1)
    mov     eax, 2
    mov     dx, PORT_DOORBELL
    out     dx, eax
    in      eax, dx                      ; defined 0
    test    eax, eax
    jnz     .fail_d
    mov     al, 'D'
    call    putc

    ; ---- success --------------------------------------------------------
    mov     al, 'X'
    call    putc
    ret

.fail_c:
    mov     al, 'c'
    jmp     .fail_out
.fail_e:
    mov     al, 'e'
    jmp     .fail_out
.fail_b:
    mov     al, 'b'
    jmp     .fail_out
.fail_d:
    mov     al, 'd'
.fail_out:
    call    putc
    ret                                  ; crt0 parks in HLT

; putc: AL -> debug serial. Clobbers DX only.
putc:
    mov     dx, SERIAL_PORT
    out     dx, al
    ret

SECTION .bss
align 8
vns_sample: resq 1
align 8
ent_buf:    resb 32
align 8
blk_wbuf:   resb 512
align 8
blk_rbuf:   resb 512
