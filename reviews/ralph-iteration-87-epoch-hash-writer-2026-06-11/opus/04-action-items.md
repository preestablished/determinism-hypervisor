# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **(S1) Optionally hoist the duplicated `epoch` binding.** `crates/dh-vmm/src/runctl.rs:337`
  and `:375` each compute `let epoch = seg.config.epoch_len.max(1);`. Both are correct; hoisting a
  single binding near the top of the loop body removes the duplication. Non-blocking, taste-level.

- [ ] **(S2) Optionally add `debug_assert_eq!(icount % epoch, 0)` before each sink push.**
  `crates/dh-vmm/src/runctl.rs:338` and `:397`. The grid-alignment invariant is true by
  construction today but unguarded; a debug assertion would catch a future scheduling refactor that
  breaks it before it silently corrupts `epoch_index`. Defensive only.

- [ ] **(S3) Optionally document the empty-slice no-op contract.**
  `crates/dh-vmm/src/recording.rs:73-84` — note in the doc comment that `log_epoch_hashes(&[], rip)`
  writes nothing and does not set `FLAG_EPOCH_HASHES`, so a quantum crossing no epoch boundary stays
  flag-free. Helps the 39w replay author who pairs with this. Doc-only.

---

## Follow-up context (NOT defects in this change — for session hand-off)

- This change (`y62`) lands the **producer** side only. Bead **`a5e`** ("every EPOCH_HASH equal,
  x100") still depends on **`determinism-hypervisor-39w`** (OPEN), the replay executor that must
  "verify EPOCH_HASH records against the live chain as it goes" — i.e. the READ/verification side.
  39w is not in scope here and remains open. y62 unblocks a5e's producer gap but a5e cannot start
  until 39w also lands. No action required in this PR; recorded so the next session knows a5e is
  still blocked on 39w.

- When closing the y62 bead, note in the close reason that the producer path is proven by
  `epoch_hashes_flow_from_quantum_to_sealed_log` and that 39w remains the gating dependency for a5e.
