# DHSNAP v1 Container Codec — Review Overview

- **Branch:** `ralph/iteration-64-dhsnap-v1-container-codec` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** 68l — DHSNAP v1 container codec in `dh-snapshot`

## Summary

This change replaces a 9-line stub with a complete, spec-faithful DHSNAP v1
container codec: a 16-byte header (`DHSNAP` magic, version `0x0100`,
`section_count u32`, `_pad u32`), back-to-back fixed-layout sections
(`tag[4]`, `sec_version u16`, `_pad u16`, `len u32`, contents, zero-pad to 8),
the full 11-tag table, a single-source device-id↔tag map, and typed
`TimeSection`/`EntrSection` structs for the two engine-owned fixed sections.
`Container::parse` is a total decoder over untrusted bytes — I traced every
slice index to a dominating bounds check and could not construct a panicking
input; the `arbitrary_truncations`/`single_byte_corruptions` smoke tests
corroborate this. The codec correctly owns only FRAMING + tag table + id map +
TIME/ENTR typing, leaving section contents to their owners, so no KVM types
leak in and aarch64 builds stay clean. Every layout claim in API.md §4 is
matched byte-for-byte, all seven device IDs cross-check against the live
`DEVICE_ID_*` constants, and `EntrSection`'s field order mirrors
`dh-devices::entropy::EntropyState` exactly. The work is high quality and
merge-ready; my one substantive concern is a **documentation gap**, not a code
defect: the genuine ENTR semantic conflict (the entropy *device* emits a
16-byte MMIO-register blob, but `tag_for_device_id(0x0004) → ENTR` whose typed
section is the 56-byte PRNG state) is acknowledged in-code only obliquely and
should be pinned to bead 6yl explicitly so it cannot be silently lost.

## Verdict

**APPROVE** — merge-ready. The ENTR/6yl tension and the 0x0007/mmv anticipation
are correctly out of this bead's scope; both are flagged below as Important
documentation follow-ups (non-blocking for this codec, but must be tracked).

## Stats

| Metric | Value |
|---|---|
| Files changed | 3 (`src/dhsnap.rs` +353, `src/lib.rs` +2, `tests/dhsnap_codec.rs` +329) |
| Net lines | +684 |
| Tests | 17 (codec) — all passing |
| `cargo test -p dh-snapshot` | PASS (17 + 1 readiness + 0 doctests) |
| `cargo clippy -p dh-snapshot --all-targets` | clean (0 warnings) |
| Critical findings | 0 |
| Important findings | 2 (both documentation/tracking) |
| Suggestions | 5 |
