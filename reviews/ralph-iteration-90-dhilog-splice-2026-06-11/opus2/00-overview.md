# Iteration 90 — DHILOG Lineage Splice — Second-Reviewer Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-90-dhilog-splice`
- **Scope:** `crates/dh-inputlog/src/splice.rs` (new, 276 lines), `crates/dh-inputlog/src/lib.rs` (+1 `pub mod splice;`). Total: 2 files, +277.
- **Bead:** `determinism-hypervisor-3lt` (DHILOG concatenation/splicing); consumer `determinism-hypervisor-cw2` (M7 1000-fork harness).

## Summary

This change adds `Lineage`, a borrowed, validated fork-tree *path* over sealed DHILOG v1
segments. It does NOT byte-concatenate; it proves a sequence of independently-sealed v1
logs composes — each segment fully re-validated through `LogReader::parse`, then four
continuity rules enforced at construction: (1) one machine (`machine_config_hash` + clock
ratio identical to segment 0), (2) the stitch rule `seg[i].end_snapshot_id ==
seg[i+1].base_snapshot_id`, (3) inner segments must carry a non-zero end snapshot (zeros =
"no end snapshot", legal at the leaf only), and (4) non-empty. It exposes `edges()` (the
per-edge `VerifyReplay(base, log)` plan, root-first), `root_base()`, `end_identity()`, and
an `extend()` fork-composition helper.

The module is correct, total over hostile input (it inherits the reader's panic-free decode
discipline — confirmed against `~/.claude/research/rust-nostd-wire-codecs.md`), and the
doc-comment is unusually precise about *why* a splice is a validated sequence rather than a
concatenated file. The first reviewer's findings are not in front of me, but I focused on
the consumer-integration gaps and the hostile-lineage shapes that a content-addressed
induction proof must survive.

The one finding I would not let ship without a decision is **ergonomic, not a correctness
bug**: `Lineage` is not `Clone` while `extend()` consumes `self`. The named consumer (cw2)
forks 1000 children off ONE parent prefix. Today each child must rebuild the prefix via
`new()` — 1000 full re-validations (each = N `blake3` body-hash recomputations over the
prefix segments). That is an O(children × prefix) blowup on the exact hot path this module
exists to serve. Deriving `Clone` (every field already is) turns it into O(children).

No correctness defect found. The self-loop / 2-cycle / duplicate-segment hostile shapes are
all harmless (a lineage is a path, never a graph — see 01). The boot-rooted zero base is
legal-by-design but undocumented and unflagged for the consumer (suggestion).

## Verdict

**APPROVE WITH NITS.** The code is correct and the validation is sound. Ship after deciding
the `Clone`/`extend` ergonomics (Important — it directly degrades the consumer's hot path)
and, ideally, landing the leaf-only single-segment test (the simplest cw2 case is currently
untested). Everything else is a suggestion.

## Stats

| Metric | Count |
|---|---|
| Critical findings | 0 |
| Important findings | 1 |
| Suggestions | 7 |
| Positive notes | 6 |
| Files reviewed | 5 (splice, reader, dhilog, verify_replay, snapstore-types) |
| Tests run | `cargo test -p dh-inputlog --lib splice` → 3 passed |
