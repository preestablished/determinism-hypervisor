# Review: DHILOG lineage splicing (bead 3lt)

- **Branch:** `ralph/iteration-90-dhilog-splice`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Scope:** 2 files, +277 / 294 diff lines, 1 commit
  - `crates/dh-inputlog/src/lib.rs` (+1: `pub mod splice;`)
  - `crates/dh-inputlog/src/splice.rs` (new, 276 lines)

## Verdict

**APPROVE** (with one Important ergonomics finding for the named consumer and a few Suggestions — none are correctness blockers).

## Summary

`splice.rs` implements the INTEGRATION §3 verification model as a host-runnable, zero-new-dep
module. The design thesis holds up against the normative docs: a splice is a **validated
sequence of sealed DHILOG v1 segments**, never a byte-concatenated file. `Lineage::new`
parses every segment through the reader's full validation battery (sealed-only, watermark,
body_hash, END identity — confirmed in `reader.rs`), then enforces the cross-segment
continuity rules:

- **Stitching** (`seg[i].end_snapshot_id == seg[i+1].base_snapshot_id`) matches
  INTEGRATION §3's induction diagram and API.md §3.4(3) exactly.
- **No dead-end inner segment** (inner `end_snapshot_id == [0;32]` rejected) correctly
  encodes API.md §3.1's "zeros if no end snapshot was taken" as leaf-only.
- **One machine** (`machine_config_hash` + clock ratio identical across the lineage).
- **icount/seq restart per segment by design** — confirmed correct against API.md §3.4(3):
  "Each log's icounts restart at 0 from its own base; there is no global icount." The
  module is right to NOT invent a cross-segment watermark rule.

The continuity case analysis is **complete**: a hostile zero-to-zero stitch is impossible
because the inner-`end == 0` check runs *before* the `end == next.base` comparison, so any
inner segment that could only stitch via a zero ref is rejected first. A `[0;32]`
`base_snapshot_id` (BOOT segment) is legal only at the root, and falls out of the stitch
rule automatically everywhere else.

Tests are strong: a 3-segment lineage validates and plans edges in root-first order;
`extend` composes and refuses strangers; and every violation (empty, broken stitch,
dead-end inner, config/clock mismatch, corrupt segment with correct index) is asserted
loud. All 3 tests pass; `cargo clippy -p dh-inputlog` is clean.

## Verification performed

- Read `/tmp/iter90.diff` and `splice.rs` in full.
- Cross-checked against INTEGRATION.md §3 (lines ~113-145) and API.md §3.1/§3.4.
- Confirmed `reader.rs::LogReader::parse` runs sealed-only + watermark + body_hash + END
  validation, so `Lineage` inherits per-segment validation as claimed.
- Confirmed `bd show determinism-hypervisor-3lt` (IN_PROGRESS) and `-cw2` (OPEN, blocked
  on 3lt) — cw2 is the 1000-children VerifyReplay consumer; no consumer code exists yet.
- Ran `cargo test -p dh-inputlog --lib splice::` → 3 passed.
- Ran `cargo clippy -p dh-inputlog` → clean.

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 1 |
| Suggestions | 4 |
| Positive notes | 6 |
