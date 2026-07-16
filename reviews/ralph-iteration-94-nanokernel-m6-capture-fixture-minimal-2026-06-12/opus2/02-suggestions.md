# Suggestions (non-blocking)

## S1 — No `FbInfo`/descriptor at the framebuffer start; C4 capture would have nothing to parse

**File:** `asm/capture_fixture.asm` (framebuffer fill, lines ~95-103); `src/lib.rs`
(`CAPTURE_FIXTURE_FB_QWORD_BASE`).

ARCHITECTURE §6.8 and API.md C4 say a `FRAMEBUFFER`-flagged region begins with a small
descriptor struct `{width, height, stride, pixel_format}` (surfaced as `FbInfo`), and the
capture engine reads pixels *after* it. This fixture's framebuffer is pure known-pattern
content from byte 0 — the first qword is `0xFB00000000000000`, not a valid descriptor. That
is entirely correct for what this fixture targets (the **C2 by-name `ExtractRange` /
layout_version path** and the **C5 capture-neutrality** acceptance, both of which read raw
region bytes), but a future C4 `CaptureSpec.framebuffer = true` path exercised against this
guest would parse garbage dimensions.

**Why mention it:** the module header and `src/lib.rs` doc both call this "the M6
capture-engine fixture" without scoping out C4. A one-line note prevents a future reader from
wiring the C4 framebuffer-decode path against this guest and being surprised.

**Suggested fix:** add a sentence to the asm module header / `capture_fixture_elf()` doc, e.g.

```text
; NOTE: the framebuffer is raw known-pattern content with NO FbInfo
; descriptor at offset 0 — this fixture exercises the C2 by-name /
; layout_version and C5 neutrality paths, not the C4 FbInfo decode (§6.8).
```

## S2 — cmdline parse omits the `BOOTINFO_MAGIC` guard that `landing_loop` performs

**File:** `asm/capture_fixture.asm:48-58` (`.parse` setup) vs `landing_loop.asm:36-44`.

`landing_loop` checks `BOOTINFO_OFF_MAGIC == BOOTINFO_MAGIC` before trusting
`cmdline_len`/`cmdline`; this fixture only null-checks `BOOT_INFO_PTR` (it already read
`mem_size` from the same struct, so the pointer is trusted) and then reads `cmdline_len`
directly. In practice dh-vmm always populates a valid BootInfo, so this is harmless, but the
two parse loops are described as "same parse contract as landing_loop" in the header — they
are *not* byte-identical because of this missing guard. Either add the magic check for true
parity, or soften the comment to "same digit-parse contract (magic already implied by the
mem_size read above)".

## S3 — `layout_version` parse can silently truncate / overflow on a pathological cmdline

**File:** `asm/capture_fixture.asm:48-58`.

The parse accumulates into 64-bit `rax` (`imul rax, 10; add rax, rdx`) then narrows with
`mov ecx, eax`, storing a u32. A cmdline like `"4294967296"` wraps to 0 → falls back to the
default; a longer digit string can overflow `rax` itself. This is a non-issue for a test
fixture whose cmdline is author-controlled, and it mirrors `landing_loop`'s identical
behaviour, so I would not change the code. Worth at most a half-line comment that the knob is
trusted/author-supplied so nobody later treats this as a hardened parser.

## S4 — Interop test rebuilds the channel header inline instead of reusing `ChannelHeader::canonical()`

**File:** `tests/capture_manifest_interop.rs:29-41`.

The test writes magic, proto, flags, and the four ring descriptors by hand to mirror the asm
byte-for-byte — which is a *deliberate and correct* choice (the test's whole point is to
prove the asm's hand-rolled header bytes pass the real attach, so reusing the encoder would
weaken it). No change requested; flagging only so a future "DRY" refactor doesn't collapse
this into `ChannelHeader::canonical().write_to(...)` and quietly lose the byte-level
guarantee. A one-line comment to that effect would protect it.

## S5 — `read_region` over-read negative test reads only 8 bytes past the very end

**File:** `tests/capture_manifest_interop.rs:139-145`.

The "read past the region end is refused" case starts at `FB_BYTES - 8` with a 16-byte
buffer, i.e. it ends exactly 8 bytes past `region.len`. That correctly exercises
`end > region.len` in `read_region` (`detguest-host/manifest.rs:127`). A slightly stronger
variant would also assert the **exact-fit** boundary succeeds (read the final 8 bytes at
`FB_BYTES - 8` into an 8-byte buffer) so the off-by-one in both directions is pinned. Minor;
the full-region read already covers the in-bounds tail.
