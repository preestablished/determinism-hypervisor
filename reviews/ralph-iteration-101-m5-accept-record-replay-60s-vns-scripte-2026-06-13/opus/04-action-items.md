# Action Items

## Action Items

### Critical
- None.

### Important
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:399-402] In `assert_table_eras`, the `expected.dedup()` on the expected side asymmetrically weakens the assertion: if two scripted pads are ever adjacent-equal it collapses two eras to one, which would also mask a guest that genuinely missed an era (a real divergence). It is a no-op for the current fixed seed (verified: no adjacent-equal pads, no zeros at 6s/60s), but the module comment invites changing the seed. Either (a) drop `dedup()` on the expected side so the comparison is exact, or (b) add a `debug_assert!(script.windows(2).all(|w| w[0] != w[1]) && !script.contains(&0), ...)` at the top of `pad_script`'s consumers so the no-op is a checked invariant. The host-side reseal hammer still covers the affected leg, so this is not Critical — but it is the kind of erosion the determinism gate exists to prevent.

### Suggestions
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:88-151] `VmMem`, `gettid`, `record_bus`, and most of `config` are copy-pasted from `replay_engine.rs:41-86`. There are now two near-identical copies; consider extracting the shared rig into `tests/common/mod.rs` (which already uses the `#[allow(dead_code)]` selective-use pattern) before a third copy appears.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:276] Add a one-line comment that `frame_hint = i` is threaded through only because the reseal hammer requires it to round-trip byte-identically — it is not independently asserted in this file — so the next reader doesn't have to trace `recording.rs`/`replay_engine.rs` to confirm it's load-bearing.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:148] The entropy device base `0xD000_3000` is a bare literal while `PV_PAD_BASE` two lines up is symbolic. If `dh_devices` exports a `PV_ENTROPY_BASE`, use it; if not, file a follow-up to export one (same literal also appears in `replay_engine.rs:83` and `common/mod.rs:92`).
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:376-388] With the no-wrap guard already asserted at line 376-379, the `i & (CAPACITY-1)` masking at line 389 is defensively redundant; a plain `i * ENTRY_BYTES` would read as "no wrap possible here." Optional — the masked form is also fine and more robust if the guard ever loosens.
