# Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:362] The acceptance gate re-pins the exact DHILOG
  bytes (`assert_eq!(outcome.resealed, rec.log, "the reseal hammer")`). `replay_segment` *already*
  enforces byte-identical reseal internally (replay_engine.rs:376) and errors via the
  `.expect("replay must not diverge")` on line 353 — so this line adds no divergence-detection power,
  but it does couple the M5 gate to the log's on-disk byte layout, meaning a legitimate,
  determinism-preserving log-format/header refactor would redden the gate and force re-blessing an
  11-minute ×100 run. Either (a) drop this redundant byte compare and keep only the semantic outcome
  asserts (`records_applied`, `epoch_hashes_verified`, `end_icount`, `end_state_hash`), or (b) keep
  it but add a one-line comment recording the *deliberate* decision that the acceptance gate also
  locks the wire format. The blocking ask is the explicit decision + comment.

### Suggestions
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:369-403] `assert_table_eras` consecutive-dedups
  both observed and expected era columns, so a scripted pad whose value equals the previous latch is
  invisible (false-PASS window, negligible for SplitMix64 over ≤59 values). Soften the
  doc/assert-message from "latch eras == the script" to "latch eras (consecutive-deduped) track the
  script," and note that the strong "every PAD_SET landed" proof is the host-side `pads == script`
  compare at line 310.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:9-13 and 284-289] Tighten the module doc: the
  non-1:1 clock makes the RECORD-side `last.vns == seconds * 1e9` check valuable (catches clock/vns
  arithmetic bugs), but the REPLAY-side `end_vns` check (replay_engine.rs:327) cannot fail
  independently of the separately-asserted `end_icount` once the clock ratio is header-verified.
  Reword "first gate to exercise the end_vns check unmasked" so a future reader does not treat the
  replay-side `end_vns` as an independent oracle.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:88-94] `gettid()` + its `unsafe` block is
  duplicated verbatim across this file, replay_engine.rs:41, and recording.rs:551. Consider hoisting
  a `pub fn gettid()` into `tests/common/mod.rs`. (DRY only — the SAFETY comment is correct.)
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:326-364] `replay_once` returns `SlotVm` solely so
  the caller can read the guest observation table. Add a one-line doc on the return value so a future
  cleanup does not change it to `()` and break `assert_table_eras`.
