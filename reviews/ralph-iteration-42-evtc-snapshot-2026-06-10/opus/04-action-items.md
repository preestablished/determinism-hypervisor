# Action Items

Self-contained checklist for `ralph/iteration-42-evtc-snapshot`
(`crates/dh-devices/src/detchannel.rs`). None block merge.

### Critical

_None._

### Important

- [ ] **I-1 — Document the responder/`FaultPlan` non-serialization.** `restore()`
  silently does not serialize or reset `responder: InjectResponder<P>`, whose
  production plan `TableFaultPlan` (guest-sdk `inject.rs`) carries accumulating
  `hits: Vec<u32>` and `decisions: Vec<(u32, FaultDecision)>`. This is the only
  non-serialized field without a "NOT serialized, by design" note. Add a doc
  sentence to `snapshot()`/`restore()`: the responder's plan state is intentionally
  not serialized — the orchestrator supplies the plan at construction (replay:
  log-backed, self-reconstructing; recording: the synthesizer's plan), so `restore()`
  adopts the fresh host's responder unchanged. (Doc-only; no behavior change
  expected for the M4 replay path.)

### Suggestions

- [ ] **S-1 — Refuse out-of-range presence flags.** In `restore()`, decode
  `bytes[12]`, `bytes[17]`, `bytes[22]` strictly (`0`/`1` only; anything else →
  `Err(RestoreError)`) instead of treating non-`1` as absent. Matches the
  "refuse loudly" posture already used for the bad-header path. Add a test feeding a
  flag byte of `2`.
- [ ] **S-2 — Move `EVTC_LEN`/`EVTC_VERSION` to free module consts.** They are
  independent of `M, P`; as inherent associated consts they force
  `DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN` turbofish at call sites
  (visible in the tests). Free `pub const`s remove the wart and read cleaner for the
  future dh-snapshot framing consumer.
- [ ] **S-3 — Note the fresh-host metrics precondition.** `restore()` only
  increments `metrics.manifest_read_failures` and never zeroes metrics; document
  that restore assumes a freshly-constructed host (metrics zeroed by `new`), or zero
  them in the validated-assignment block, to avoid a reused-host accumulation
  footgun.
