# Positive Notes

## P1 — Drift pin cross-checks against the real codec, not re-typed literals

`tests/elf_shape.rs` (`capture_fixture_asm_matches_rust_constants`, lines ~330+) compares the
asm `%define`s for `MANIFEST_MAGIC`, `MANIFEST_OFF`, `OFF_ENTRY0`, `OFF_EXTENT0`, and
`REGION_FLAG_FRAMEBUFFER` against `detguest_wire::manifest::{MANIFEST_MAGIC, RegionEntry::offset(0),
Extent::offset(0), REGION_FLAG_FRAMEBUFFER}` and `detguest_wire::header::OFF_MANIFEST` — the
actual wire codec symbols. This is exactly the research-context guidance ("derive mirrors from
shared constants, not re-typed literals"): if the wire format ever moves an offset, this asm
fixture's stores fail compilation rather than producing a subtly-wrong manifest at runtime.
The comment ("Wire-format truth from detguest-wire, not re-typed literals") makes the intent
explicit.

## P2 — Host-runnable interop test exercises the REAL host code over guest-authored bytes

`tests/capture_manifest_interop.rs` does not re-implement the manifest reader; it builds the
channel page **byte-for-byte the way the asm does** (down to leaving generation, region_id,
gva, extent_off, the other 63 slots, and name padding zeroed) and then runs the genuine
`Channel::attach`, `read_manifest`, `resolve`, and `read_region`. This makes it a true
contract test: it proves the guest's hand-rolled bytes survive the host's bounds-checking and
seqlock-consistent reader, which is the load-bearing guarantee until the guest can be executed
on hardware. The three cases (attach+resolve, full/unaligned/over-read region walk, bumped
layout_version visible) cover the C2 and C5 acceptance hooks cleanly.

## P3 — Generation/seqlock reasoning is correct and documented

The decision to write the manifest *before* `CHANNEL_INIT` (leaving generation 0/even) and
skip the seqlock writer dance is both correct (the host's first read is at attach, after all
stores land) and explained in the module header. The interop test asserts `generation == 0`
and the negative seqlock behaviour is already covered upstream in `detguest-host`.

## P4 — Clean-room provenance and doc-contradiction handling carried over faithfully

The fixture reuses `device_exercise`'s canonical channel header verbatim (including the
deliberate `W size = 0x100000` power-of-two choice that resolves the ARCHITECTURE-table /
attach-validation contradiction documented in `detguest-wire/header.rs:93-103`). Reusing the
already-reconciled value rather than re-deriving it is the right call and keeps both guests
attaching identically.

## P5 — Constants and accessor are well-documented and self-consistent

`src/lib.rs` adds the GPAs, pattern base, default layout_version, region name, and OK
sequence with doc comments that explain *why* (e.g. "Requires mem_size >= GPA + BYTES",
"Mirrors the asm %define (drift-tested)"). The `lib.rs` smoke test and `elf_shape` guest-shape
list were both updated, so the new guest is non-empty-checked and shape-checked alongside the
others. Nothing was half-wired.

## P6 — Bounds-aware test assertions

The interop test verifies an interior unaligned slice agrees with the full read
(`slice == buf[1234..1334]`) and that an over-read is **refused, not truncated** — matching
the host's `end > region.len` rejection. Pinning "refused, not truncated" is the kind of
assertion that catches a future silent-clamp regression in `read_region`.
