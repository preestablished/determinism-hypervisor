# Action items

### Critical

_None._

### Important

- [ ] **Fix `first_bad_epoch` for non-epoch-chain divergences.**
  In `crates/dh-worker/src/verify_replay.rs:77`, `first_bad_epoch = at_icount /
  epoch_len.max(1)` is only correct when `what` is `"EPOCH_HASH chain value"` or
  `"EPOCH_HASH the recording does not have"`. For `"end_state_hash"`, `"end_vns"`,
  and `"EPOCH_HASH count ..."` the `at_icount` is `end_icount`, so the division names
  an epoch that actually *matched* (`first_bad_epoch == total_epochs`). For
  `"resealed log bytes ..."` the `at_icount` is a **byte offset** (replay_engine.rs:381),
  so the division is meaningless. Make the mapping `what`-aware: only the two
  epoch-chain `what`s get the division; the END-class divergences report
  `first_bad_epoch = expected_epochs.len()` *with text making clear all epochs passed
  and the divergence is post-epoch*, or — cleaner — change the field to `Option<u64>`
  and emit `None` for non-epoch-localized divergences; the resealed-byte case must
  never divide a byte offset. Extract the mapping into a pure function (see S4) so it
  can be unit-tested without KVM.

- [ ] **Correct the proto-fidelity claims and reconcile the model's `Divergence`.**
  `crates/dh-verify/src/verify.rs:11,21` claims the model mirrors proto §2.7
  `VerifyReplayProgress`/`Divergence` and that only the M8 bisection fields are
  deferred. In fact the model's `Divergence { first_bad_epoch, at_icount, what,
  expected, got }` shares only `first_bad_epoch` with proto `Divergence`
  (hypervisor.proto:340-349); `at_icount`/`what`/`expected`/`got` have no proto
  counterpart and the proto carries no hash pair. Update the doc comments to state
  the truth: this is the **library** verdict shape from bead 1py (first bad epoch +
  the diverging hash pair), and rfv will *translate* it into proto `Divergence`
  (mapping `what` → `suspected_cause`, leaving M8 bisection fields empty; the hash
  pair is library-only). `EpochOk`/`VerifyDone` *do* mirror the proto — leave those
  claims. If a tighter model↔proto correspondence is wanted for rfv, decide that now,
  since cw2/rfv build on this surface.

- [ ] **Promote the EpochOk-count check from `debug_assert_eq!` to a hard check.**
  `crates/dh-worker/src/verify_replay.rs:63` pins `emitted ==
  outcome.epoch_hashes_verified` only in debug builds. Release builds (cw2's 1000x
  harness, rfv's production RPC) would silently misreport the epoch count on any
  re-parse/engine disagreement. Return an `Err` (an infrastructure inconsistency, not
  a verdict) when the counts differ. Cost is one comparison per run — negligible.

### Suggestions

- [ ] **Add a harness summary affordance to `VerifyReport`** (`verify.rs:38-71`) for
  cw2's 1000-row table: a `{ verified, first_bad_epoch, what }` summary and/or a
  `gate.rs`-style `artifact()`/`Display`, so the consumer does not reinvent
  `match`-ing over `events`. (S1)
- [ ] **Document the "exactly one terminal event" invariant** on `VerifyReport`, and
  note `verified()`'s `divergence().is_none()` is defensive for hand-built reports.
  (S2)
- [ ] **Drop or comment `epoch_len.max(1)`** (`verify_replay.rs:77`) — config
  validation already rejects `epoch_len == 0` before any divergence can be produced,
  so the guard is dead. (S3)
- [ ] **Extend test coverage** to the END-class and resealed-byte divergence kinds
  (host-runnable if the mapping is extracted per the I1 fix), since the current live
  test exercises only the one `what` whose arithmetic is correct. (S4)
- [ ] **Tighten the "maps 1:1" / proto-mirror doc comments**
  (`verify_replay.rs:11`, `verify.rs:11,21`) to match the actual (non-1:1)
  translation. (S5)
