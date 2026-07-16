# Suggestions (non-blocking)

These are quality/robustness notes. None block the merge once C1 is fixed.
Several are deliberate-and-documented tradeoffs I am only flagging so the
choice is on the record.

### S1 — `BOOT_INFO_PTR` is dereferenced for `mem_size` with no BootInfo magic/version check

**File:** `device_exercise.asm:130-136`

The `D` stage gates on `mem_size >= CHANNEL_GPA + 2 MiB`:

```asm
mov     rsi, [BOOT_INFO_PTR]
test    rsi, rsi
jz      .fail_d
mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
cmp     rax, CHANNEL_GPA + 0x200000
jb      .fail_d
```

The null check is good. But unlike the spirit of `bootinfo.inc` (which
defines `BOOTINFO_MAGIC`/`BOOTINFO_VERSION` precisely so consumers can
validate the struct), the program reads `mem_size` unconditionally. If the
VMM ever placed a non-null but garbage pointer in RSI, `mem_size` is garbage
and the stage's pass/fail is non-deterministic w.r.t. the *intended* contract
(though still a deterministic function of the actual bytes — so replay-safe,
just not contract-checked). For an *acceptance* guest this is the natural
place to also assert `[rsi + 0] == BOOTINFO_MAGIC` and `[rsi + 4] ==
BOOTINFO_VERSION` before trusting `mem_size`. Cheap, and it documents the ABI
dependency the way the rest of the file documents device ABIs. (`landing_loop`
sets a precedent worth matching — its `elf_shape` test cross-checks the inc
file against Rust constants; a magic check would make that ABI link load-bearing
at runtime too.) Low priority: in the real VMM the pointer is valid, so this
is belt-and-suspenders.

### S2 — The `'P'` pad stage is a pure presence test and can never fail

**File:** `device_exercise.asm:99-103`

```asm
mov     rbx, PAD_BASE
mov     eax, [rbx + PAD_PAD0]   ; latch readable (any value)
mov     al, 'P'
call    putc
```

There is no `.fail_p` and no comparison — the read result is discarded and
`'P'` is emitted unconditionally. The module header is honest about this
("latch read ... presence test"), and since pad values are host-injected
there's nothing deterministic to assert. Fine as-is, but worth a one-line
comment at the call site (not just the header) noting "no failure path: a
fault here would #GP/triple-fault, not emit lowercase 'p'", so a future reader
doesn't add a bogus `.fail_p` expecting the MMIO read to signal an error.

### S3 — `mov al, 'C'` after a 64-bit `mov rax, [vns_sample-store]` relies on AL aliasing RAX; intentional but easy to misread

**File:** `device_exercise.asm` (every `mov al, 'X'` before `call putc`)

Each progress byte is set via `mov al, <char>` which writes only the low 8
bits of RAX, leaving the upper bits as whatever the stage left there (e.g. the
ICOUNT/VNS sample, the entropy OR-accumulator, the 0xB10C… fill seed). `putc`
only touches DX and reads AL, so this is correct. But because several stages
leave meaningful data in the upper RAX bytes, a reader scanning for "is AL
clean here?" has to reason about it each time. Non-issue functionally;
consider `movzx`/`xor eax,eax; mov al,...` only if you want the upper bits
provably zero for debug-dump readability. Not worth churn.

### S4 — Beacon `vnanos` uses the pv-clock VNS sampled at the very start (`'C'` stage), not a fresh read

**File:** `device_exercise.asm:147,196` (`mov [vns_sample], rax` then `mov
rax, [vns_sample]`)

The Beacon's `vnanos` field is the VNS captured during the `'C'` stage, stored
in `.bss` and reloaded ~190 instructions later. The host drain treats `vnanos`
as opaque (`hdr.vnanos`, no validation — confirmed in `drain.rs:299` and my
scratch run, which fed `0xDEADBEEF` and got it back untouched), so any value
is legal. Using a stale sample is fine and deterministic. Only flag: the
comment says "vnanos (sampled)" which is accurate but could note "sampled at
the 'C' stage, not here" so it's clear the value intentionally predates the
Beacon by the whole program. Cosmetic.

### S5 — Magic literal `0x5453455547544544` is hand-encoded; pin it to the SDK constant in a comment or test

**File:** `device_exercise.asm:140`

The magic is correct (`objdump` shows `44 45 54 47 55 45 53 54` = "DETGUEST"
LE, and `detguest_wire::header::CHANNEL_MAGIC = 0x5453_4555_4754_4544` matches
exactly — verified). But it's a 64-bit literal a human byte-swapped by hand;
if anyone "fixes" it to the more readable `0x4445_5447_5545_5354` they'll
silently break attach. The `magic_bytes_spell_detguest` test in `header.rs`
guards the SDK side; consider a parallel assertion in this repo (part of the
I1 attach test) that the guest's magic word round-trips to `b"DETGUEST"`, so
the literal is machine-checked rather than trusted.
