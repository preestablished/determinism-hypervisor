# Action items

### Critical

- [ ] **Fix multi-vector injection chaining (C1).** `run_segment`'s inner injection loop
  (`crates/dh-vmm/src/runctl.rs` 191-212) queues a second vector at the same boundary
  *without entering the guest between queues*. Because `KVM_INTERRUPT` silently overwrites
  an un-consumed queued vector (documented in `inject.rs` 96-99), all but the last vector
  at a shared boundary are **lost** — a silent, determinism-preserving wrong result.
  Choose one:
  - enter the guest one retirement (`land_at(at.icount + 1)`) after each `queue_interrupt`,
    update `at` from the post-entry boundary; or
  - push the "one vector per entry" contract into `inject_at_boundary` so it consumes the
    queue before returning; or
  - **interim:** reject `point.injections.len() > 1` with a loud `RunError` so the case
    cannot ship silently until the device loop needs it.
- [ ] **Delete or correct the false comment** at `runctl.rs` 191-193 ("inject_at_boundary
  steps between queued vectors"). It does not, on the injectable-true path.
- [ ] **Add a live two-vector test:** schedule two vectors at one boundary on an IF=1
  guest, assert BOTH deliver and the second's `delivered_icount == first + 1`, run twice
  for replay identity.

### Important

- [ ] **Honor `hash_epochs = FinalOnly` (I1).** `runctl.rs` 170-176 always passes the
  epoch grid; it ignores `seg.config.hash_epochs`, which IS part of the config-hash
  preimage. Map `FinalOnly → epoch_len: None`. Add a test that `FinalOnly` produces a
  chain with no epoch links. Coordinate with the pause grid (S1).
- [ ] **Hash a coincident epoch+final boundary exactly once (I2).** `runctl.rs` 218-223
  (epoch arm) and `finish()` 277-279 both `push_final_link` when a point is both an epoch
  boundary and the final stop → two chain links where §8.5 prescribes one → cross-impl
  chain divergence. Skip the `finish()` link when the stop boundary already hashed this
  iteration. Add a test pinning the §8.5 link count at a budget-on-epoch-multiple landing.

### Suggestions

- [ ] **Clamp pause roll-forward to the budget (S1):** `next_epoch.min(final_icount)` in
  `runctl.rs` 240-241, or document that an external pause may run up to `epoch_len`
  instructions past the requested budget (with default 50M, that overrun is large).
- [ ] **Harden `dh-cli` `gettid()` (S2):** `tools/dh-cli/src/run.rs` 13-17 is correct only
  on the main thread. Add a main-thread precondition to the `run` doc, or derive the real
  tid without `unsafe` (e.g. via `/proc`), so a future non-main-thread caller doesn't
  misroute the PMI kick.
- [ ] **Complete the StopReason subset deliberately (S3):** dh-cli `on_exit` turns
  `Hlt`/`Shutdown` into a fatal `RunError` rather than `GUEST_HALTED`/`FAULTED`. Note
  these as explicit Phase-1-out-of-scope in the module doc (as NextSdkEvent/FrameBudget
  already are) so the subset is closed on purpose.
- [ ] **Make `injections_delivered` count true deliveries (S4):** currently counts queue
  attempts; resolved as a corollary of the C1 fix (or trivially correct under the
  reject-len>1 interim).
- [ ] **Drop or refresh the stale `rcx` on chained boundaries (S5):** `runctl.rs` 207-211
  carries the pre-injection `rcx` onto the synthetic post-injection boundary; diagnostics
  only, lowest priority.
