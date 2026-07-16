# Action Items

### Critical

None.

### Important

- [ ] **I1 — Replace the EPOCH_HASH divergence string-match with a structured side channel.**
  In `crates/dh-worker/src/replay_engine.rs`, the sink (lines 365-389) signals an epoch mismatch
  by returning `BoundaryError::Exit(format!(...))`, and `run_to` (lines 391-401) classifies it
  with `m.contains("EPOCH_HASH")`, then zeroes `expected`/`got` and reports the quantum *start*
  icount instead of the link icount. Add a `Cell<Option<(u64 idx, u64 icount, [u8;32] expected,
  [u8;32] got)>>` owned by `replay_segment`; have the sink write it on mismatch and return a
  bare abort error; have `run_to`'s error map read the Cell to build a `Divergence` with the real
  icount and both hashes. This (a) removes the fragile substring dependency and (b) gives bead
  **1py** (VerifyReplay) the structured `{first_divergent_epoch, hashes}` it is specified to
  report. Do this before 1py is written against the degraded shape.

- [ ] **I2 — Make the reseal-mismatch `Divergence` carry a comparable hash pair.**
  In `crates/dh-worker/src/replay_engine.rs:492-499`, the reseal-failure path reports
  `expected = header.body_hash`, `got = [0;32]` — not the same quantity, so the pair is
  undiffable and `got` looks like a missing value. Compute the resealed body's BLAKE3
  (`*blake3::hash(&resealed[HEADER_LEN..]).as_bytes()`) for `got`, or introduce a dedicated
  variant that carries enough to localize which record differs (the reseal hammer catches AUX
  divergences — FRAME_MARK / NET_TX / ENTROPY ordering or icount — that the per-record checks
  never inspect, so this is the variant most in need of a usable diagnostic).

### Suggestions

- [ ] **S1 — (defense-in-depth, low priority) Optionally cross-check `out.boundary.rip ==
  rec.boundary_rip()`** after each input-landing `run_to`, in
  `crates/dh-worker/src/replay_engine.rs`. The hash chain already folds rip (and all GPRs) at
  every boundary via `push_final_link` (`crates/dh-vmm/src/hash.rs:181-208`), so a rip divergence
  is already caught by the next EPOCH_HASH / `end_state_hash`. The cross-check only adds value for
  a rip divergence at a non-epoch boundary that fully reconverges before the next grid point — an
  extremely narrow window. Two lines if wanted; not a correctness fix.

- [ ] **S2 — Comment the rip asymmetry.** At the `let rip = rec.boundary_rip();` sites
  (`replay_engine.rs:415`), note that the input rip comes from the log to reproduce the
  recording's record bytes, while the machine's actual rip at that icount is verified through the
  hash chain, not here.

- [ ] **S3 — `#[derive(Debug)]` (and likely `Clone`) on `ReplayOutcome`**
  (`replay_engine.rs:242-250`). Free; unblocks `assert_eq!`/`dbg!` on the whole struct and any
  caller (1py) that wants to log or thread it.

- [ ] **S4 — Document end-snapshot scope.** Add a one-line module note that the reseal copies
  `header.end_snapshot_id` for byte-identity and does NOT re-take or re-validate an end snapshot
  of the replayed machine — out of scope for the phase-1 verify path. Prevents a future reader
  from mistaking the byte-identical reseal for end-snapshot re-validation.
