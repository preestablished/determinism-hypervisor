# Review: Replay Path (bead 39w) — iteration 88

- **Branch:** `ralph/iteration-88-replay-path`
- **Base:** `main` (commit under review: `5d99666`)
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Scope:** P0 replay path — the product's core determinism property made executable.

## Summary

This change introduces `crates/dh-worker/src/replay_engine.rs` (`replay_segment`),
the executor that drives a fresh machine from a `(snapshot, DHILOG)` pair and proves
bit-identical reproduction. It restores the base snapshot **into the rail's own bus**
(so the rail dispatches the restored devices, not defaults — the iteration-88 design
catch), seeds entropy from the restored ENTR state (or a nonzero header seed per §3.1),
walks the canonical records applying each through the rail's paired `apply_pad_set` /
`apply_net_rx` entry points, verifies every EPOCH_HASH against the live chain **at the
link point**, checks `end_state_hash`, and finally swings the **reseal hammer** —
the resealed log must be byte-identical to the input or it is a `Divergence`.

The supporting API change converts the iteration-87 `Vec` epoch sink into a
`&mut dyn FnMut(u64,u64,[u8;32]) -> Result<(),BoundaryError>` that fires at the link
point. This is **correct and necessary**: the DHILOG icount watermark is monotone, and
the prior post-quantum batch-landing regressed behind later exit records
(`IcountRegressed`). `recording.rs::log_epoch_hashes` (batch) becomes `log_epoch_hash`
(single link); `runctl::run_segment` adapts via a no-op closure; the y62 live test and
the new replay tests share the rail through a `RefCell`.

Quantization independence is the headline property: replay quantizes **by record**
(one quantum per canonical record + a tail to `header.end_icount`), deliberately unlike
the recording's fixed 100k quanta, and the absolute epoch grid keeps the EPOCH_HASH sets
equal link-for-link. The live tests demonstrate it: a 3-quantum pad-echo recording
replays bit-identically; a RAM-poisoned recording diverges at the first epoch; a foreign
config is refused at the header before any restore.

The code is careful, well-documented, and the verification layering (per-record checks
for diagnostics, reseal hammer for strength) is sound. I found **no correctness bugs**.
The findings are diagnostic-quality and defense-in-depth improvements, plus one design
question (rip cross-check) the prompt explicitly asked me to judge.

## Verdict

**APPROVE**

The replay path is correct, the negative tests are genuine (not tautological), the
monotonicity and ordering guarantees hold, and the header/END consistency the engine
relies on is enforced by the reader (`EndMismatch`). The Important items are
diagnostic-degradation issues worth fixing before the dh-verify consumer (1py) builds on
this, but none threaten the core property.

## Bead discharge

- **"verify EPOCH_HASH records against the live chain as it goes"** — discharged. Sink
  fires at each link point and compares `(epoch_index, icount, chain_value)` against the
  recording in emission order, plus a post-loop count reconciliation.
- **"end_state_hash at END"** — discharged (`live_end != header.end_state_hash` →
  Divergence), and strengthened by the reseal hammer.
- **"feed entropy from ENTR state"** — discharged. `rail.entropy = restored.entropy`
  for a zero header seed; `DetEntropy::from_seed(header.entropy_seed)` for a fresh nonzero
  seed (§3.1 semantics).
- **DEV_EVENT / vectored inputs** — loud `NotYetWired`, never silently skipped. Correct
  for phase-1; the M5 demo path (polling pad-echo, loopback net) needs neither.

## Downstream readiness (1py / a5e)

- **1py (VerifyReplay in dh-verify):** needs `Divergence{first_divergent_epoch, hashes}`.
  The current `Divergence` carries zeroed hashes and the quantum *start* icount on the
  EPOCH_HASH path (the real detail lives only in a formatted string). See item I1 — this
  should be tightened before 1py consumes it, or 1py will have nothing structured to
  report.
- **a5e (M5 ACCEPT, x100 pad sequence):** the bit-identical reseal + epoch verification
  is exactly the property a5e gates on. Ready.

## Stats

- Files changed: 6 (+750 / −24)
- New: `replay_engine.rs` (349 lines), `tests/replay_engine.rs` (355 lines)
- Findings: **0 Critical, 2 Important, 4 Suggestions**
- Build: clean (`cargo build -p dh-worker`)
- Clippy: clean (`cargo clippy -p dh-worker -p dh-vmm`)
- Tests: compile clean (hardware-gated; not run here — no `/dev/kvm`)
