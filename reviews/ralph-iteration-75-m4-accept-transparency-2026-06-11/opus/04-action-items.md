# Action Items

### Critical

_None._

### Important

- [ ] [crates/dh-worker/tests/m4_transparency.rs:6-8 and :271-274] Scope the "device-state leak
  shows here" claim. The chain hashes `&[]` device sections (runctl.rs:318/374/404) and the
  landing loop reads no device MMIO, so entropy/clock transparency is **not** gated by H1==H2 —
  the M4 ENTR golden test owns that. Either trim "device-state" from the module doc and the final
  assertion message, OR add a guest-observable device assertion (see S1) so the wording becomes
  literally true. (See 01-critical-and-important.md I1.)

### Suggestions

- [ ] [crates/dh-worker/tests/m4_transparency.rs:259-265] Verify the restored device path: assert
  `outcome.entropy`'s next draw equals a control `DetEntropy::from_seed([9;32])` draw, and/or that
  the restored `PvClock.vns_base == r1.vns`. Directly resolves I1; costs microseconds. Skip if the
  accessors aren't on the public dev API and apply the doc trim instead. (S1)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:269-274] Replace the three field-wise post-restore
  asserts with a single `assert_eq!(r2, c2, ...)` (matching the strong `r1 == c1` form at :215),
  optionally keeping the granular asserts above as failure-localizers. Picks up `reason`,
  `injections_delivered`, `timer_fired`. (S2)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:236, 260] Add structural sanity asserts:
  `snap.pages_shipped == MEM/4096`, `outcome.pages_loaded == MEM/4096`,
  `outcome.epoch_index == HALF/cfg.epoch_len`. The `epoch_index` check guards a TIME-section
  off-by-one `cumulative_icount` wouldn't catch. (S3)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:236] Add `assert_eq!(snap.hash_chain,
  r1.state_hash, ...)` to make "the restored chain is the parent's chain" explicit. (S4)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:63-81] Note: `kvm_usable()`/`gettid()` are
  copy-pasted from regression.rs and three runctl.rs test modules. Consider a shared
  `tests/common/mod.rs` helper before more live tests land in this package. Low priority. (S5)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:243] Reword the `let _ = chain;` comment: it's a
  move-out (which correctly makes later use a compile error), not a "shadow." Pure nit. (S6)
