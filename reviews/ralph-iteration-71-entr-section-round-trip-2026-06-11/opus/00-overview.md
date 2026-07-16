# ENTR Section Round-Trip (bead 6yl) — Review Overview

- **Branch:** `ralph/iteration-71-entr-section-round-trip` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Diff:** `/tmp/ralph71-diff.txt`

## Summary

This iteration resolves the ENTR landmine flagged in iteration 64: §4's `ENTR` tag
carried only the 56-byte VMM PRNG state, leaving the pv-entropy device's 16 bytes of
guest-visible MMIO regs (`buf_gpa`, `len`, `status`) with no §4 tag of their own.
The fix introduces `EntrSectionV2` (72 bytes, `sec_version = 2`) in
`crates/dh-snapshot/src/dhsnap.rs` = PRNG state ‖ device regs, with `from_parts` /
`device_regs` / `prng` as the combine/split seam, while v1 (56 bytes) remains exactly
decodable. The landmine doc comment is rewritten from speculative to RESOLVED. A new
integration test `crates/dh-snapshot/tests/entr_roundtrip.rs` (gaining a `dh-devices`
dev-dependency) drives the full chain — live `DetEntropy` → `EntropyState` →
`EntrSectionV2` → DHSNAP container bytes → parse → decode → `DetEntropy::restore` —
and proves the M4 golden property (bit-identical next draws), including a 37-byte
sub-word fill that exercises the word-granularity invariant. The layout is correct and
matches the device's `DetDevice::snapshot`/`restore` byte-for-byte; the tests pass.
The one substantive gap is process/spec hygiene, not correctness: writing a 72-byte
`sec_version = 2` ENTR section diverges from API.md §4 (which states ENTR is
"exactly ... 56 bytes" and lists no v2 row), and that divergence is recorded only in a
code comment — not in the spec table, not in a frozen golden-bytes fixture (which §4
itself mandates "per version"), and not in any durable divergence ledger.

## Verdict

**NEEDS_DISCUSSION**

The code is correct, well-tested, and merge-ready in isolation. But the change writes
a section shape the normative spec (API.md §4) does not yet describe, and the project's
own conventions require (a) a per-version golden-bytes fixture and (b) durable recording
of spec divergences. None of those are blocking *correctness* bugs, but they are exactly
the kind of spec/producer drift this repo is built to prevent, so a human should ratify
the divergence (update §4 with a v2 row + add the golden fixture) before this is treated
as settled.

## Stats

| Metric | Value |
|---|---|
| Files changed | 4 (`Cargo.lock`, `dh-snapshot/Cargo.toml`, `dhsnap.rs`, new `entr_roundtrip.rs`) |
| Lines added | ~200 (incl. 118-line new test) |
| New public API | `EntrSectionV2` (+ `LEN`, `VERSION`, `prng`, `from_parts`, `device_regs`, `encode`, `decode`) |
| Tests added | 2 integration tests |
| `cargo test -p dh-snapshot` | PASS (18 + 2 + 4 + 1 = all green) |
| Critical findings | 0 |
| Important findings | 2 |
| Suggestions | 4 |
