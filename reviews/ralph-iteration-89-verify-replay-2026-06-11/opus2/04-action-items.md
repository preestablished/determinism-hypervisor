# Action Items

### Critical

None.

### Important

- [ ] **I-2 — Fix or document the `Divergence` field mapping.** In
  `verify_replay.rs:70-83` the uniform `first_bad_epoch =
  at_icount/epoch_len` is wrong for the non-epoch divergence shapes the engine
  emits (`replay_engine.rs`). Specifically: for `what == "resealed log bytes
  (...)"`, `at_icount` is a **byte offset** (not an icount), so dividing it by
  `epoch_len` is nonsense and `got` is a `[0;32]` placeholder; for `end_vns`,
  `expected`/`got` are LE-packed u64s, not hashes; for `end_state_hash` and
  the count-mismatch, `first_bad_epoch` points at the last epoch, not the
  divergence locus. **Do one of:** (a) make the mapping `what`-aware — only
  compute `first_bad_epoch` from `at_icount` for the `"EPOCH_HASH chain value"`
  case, use a sentinel (`u64::MAX`) otherwise; OR (b) extend the `Divergence`
  doc in `verify.rs` to state that `first_bad_epoch`/`expected`/`got` are
  meaningful only for epoch-chain and end_state_hash shapes, and that
  `end_vns` packs u64s while `resealed log bytes` carries a byte offset in
  `at_icount`. Resolve before cw2 treats these fields as triage signal.

- [ ] **I-3 — Make `verified()` order-independent.** In `verify.rs:43-47`
  replace `matches!(self.events.last(), Some(Done{..})) &&
  self.divergence().is_none()` with `self.done().is_some() &&
  self.divergence().is_none()`, and update the doc to "reached `Done` and
  carries no divergence." If "Done is terminal" is a real invariant, enforce it
  in `push` instead of encoding it implicitly in `verified()`.

- [ ] **I-1 — Make the EpochOk-count guarantee explicit.** In
  `verify_replay.rs:63`, either upgrade `debug_assert_eq!(emitted,
  outcome.epoch_hashes_verified)` to a hard `assert_eq!`, or add a comment
  pointing at the engine's count-pin (`replay_engine.rs:336`) as the real
  guarantee so a future engine refactor cannot silently weaken the
  reconstruction's honesty.

### Suggestions

- [ ] **S-1 — Document HeaderMismatch-is-Err in the wrapper.** Add a line to
  `verify_replay.rs`'s doc-comment: a mispaired (snapshot, log) is an
  infrastructure `Err`, NOT a `Divergence` verdict — cw2's zero-Divergence
  exit gate must not absorb harness wiring bugs. (Classification itself is
  correct — keep it.)

- [ ] **S-2 — File a follow-up bead for cw2's batch layer.** cw2 needs a
  per-child verdict aggregator (`VerifyBatch` of `Vec<VerifyReport>` with
  `all_verified()` / `divergences()`), placed in dh-verify to keep the
  dependency direction clean, plus a shared per-child `DeviceRail` fixture
  (today's test duplicates the rail-construction block). Nothing in this diff
  blocks it; it just doesn't exist yet.

- [ ] **S-3 — Drop or comment `epoch_len.max(1)`.** `MachineConfig::validate`
  already rejects `epoch_len == 0` (`config.rs:152`) and the config is hashed
  before any Divergence is reachable, so `epoch_len ≥ 1` is invariant here. The
  `.max(1)` masks that — drop it or comment it as belt-and-suspenders.

- [ ] **S-4 — Derive the epoch count in the test.** Replace the magic
  `assert_eq!(report.epochs_ok(), 10)` with `(3 * QUANTUM) / cfg.epoch_len` so
  a fixture tweak fails legibly.

- [ ] **S-5 — Tighten the Divergence destructure** to `assert_matches!` or a
  let-else form instead of the `match { _ => panic!() }` chain (research
  file's flagged `if let { panic!() }` pitfall).
