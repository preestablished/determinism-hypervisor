# Review: iteration 66 — memfd sealing + Frozen slot-state machine (2nd reviewer)

- **Branch:** `ralph/iteration-66-memfd-sealing-frozen-slot-state` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** determinism-hypervisor-d2p (risk R9 — fork-parent corruption / CoW aliasing)
- **Diff:** `/tmp/ralph66-diff.txt` (331 lines)

## Summary

Two halves of the R9 fork-parent guard land in this iteration:

1. **Software half** (`crates/dh-vmm/src/lib.rs`): `SlotState::can_transition` /
   `transition` (the §2.2/§8.4 transition relation) + `ensure_write_path(api)` that
   denies write-path calls on `Frozen` (and `Empty`) slots with a loud, typed error.
2. **Hardware half** (`crates/dh-vmm/src/kvm.rs`, x86-gated): `SlotVm::freeze_ram`
   applies `F_SEAL_FUTURE_WRITE | F_SEAL_SHRINK | F_SEAL_GROW` to the guest-RAM memfd,
   plus `ram_seals()` for tests/preflight, plus one live KVM test.

The implementation is clean, the experiment-critical question for the *next* bead (9e4,
the tier-A CoW fork) resolves **in the design's favor**, and all the skeptic angles
check out. The comments are unusually precise about *why* `F_SEAL_WRITE` is avoided and
why the software guard is load-bearing — that reasoning is correct and I verified it by
experiment on this box.

I did, however, find **two spec-vs-code mismatches** worth flagging (one is a real gap:
the `Faulted` slot state exists in the proto but not in the Rust state machine, and the
ARCHITECTURE lifecycle line shows `Running → Frozen` which the code — correctly —
rejects). Neither blocks d2p; both should be reconciled before the fork beads wire this
in.

## Experiments run on this box (kernel 6.8.0-124, /dev/kvm present)

- **`cargo test -p dh-vmm --lib`** → **80 passed, 0 failed**, incl. the new live
  `freeze_ram_seals_future_writes_but_not_the_live_mapping` and all 5
  `slot_state_tests`.
- **`cargo clippy -p dh-vmm --lib`** → clean, no warnings.
- **Scratch C probe** (`/tmp/sealprobe.c`) for the 9e4 CoW question — see 01 for the
  full result. Headline: **`mmap(MAP_PRIVATE, PROT_READ|PROT_WRITE)` of a
  FUTURE_WRITE-sealed memfd SUCCEEDS** on 6.8. The tier-A CoW fork design is sound.

## Verdict

**APPROVE.** No Critical or Important blockers for d2p. The two spec mismatches are
**Important follow-ups for the fork beads (9e4 et al.)**, not defects in this bead's
scope — the code's behavior is the *correct* one in both cases; it's the docs that are
stale. Suggestions are polish.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 0 (in-scope) / 2 (cross-bead spec reconciliation) |
| Suggestions| 4     |
| Files reviewed | 2 (`lib.rs`, `kvm.rs`) |
| Tests      | 80 pass, clippy clean |
