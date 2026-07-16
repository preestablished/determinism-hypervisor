# Suggestions (non-blocking)

### S1 — Duplicated rig: `VmMem`, `gettid`, `config`, `record_bus` are copy-pasted from `replay_engine.rs`
**File:** `crates/dh-worker/tests/m5_record_replay.rs:88-151` (and `replay_engine.rs:41-86`)

`VmMem` (+ its `GuestMem` impl), `gettid`, and the `record_bus` helper are
byte-for-byte the same as `replay_engine.rs`, and `config` differs only in the
config seed bytes and the added `clock`/`epoch_len`. The research note "shared
fixture code deduplicated rather than copy-pasted" applies. These could move into
`tests/common/mod.rs` (which already carries `#[allow(dead_code)]` helpers used by
only some targets — the exact pattern). Counter-argument: integration-test files are
deliberately self-contained and the divergence is small; this is a judgment call, not
a defect. Flagging because there are now *two* near-identical copies and a third would
be the moment to extract.

### S2 — The non-obvious `frame_hint = i` round-trip deserves a one-line note
**File:** `crates/dh-worker/tests/m5_record_replay.rs:276` (`script[(i - 1)]`, frame_hint `i as u32`)

`apply_pad_set(..., script[(i-1)], i as u32)` passes the second-to-last arg as
`buttons` and `i` as `frame_hint`. `frame_hint` is recorded into the log and the
reseal hammer requires it to round-trip identically on replay — but nothing in this
file reads `frame_hint` back or asserts on it, so a reader may wonder why the loop
index is threaded through. A short comment ("frame_hint is recorded and must reseal
identically; value is otherwise unobserved here") would save the next reader the trip
to `recording.rs` and `replay_engine.rs` to confirm it's load-bearing for the
byte-identity but not independently checked.

### S3 — `assert_table_eras` reads `count` frames but the ring capacity guard could be tighter
**File:** `crates/dh-worker/tests/m5_record_replay.rs:376-379`

```rust
assert!(
    count < nanokernel::PAD_ECHO_TABLE_CAPACITY,
    "ring wrapped ({count} frames) — era reconstruction would be lossy"
);
```

This correctly refuses a wrapped ring (good — see positive notes). The loop then
indexes with `i & (CAPACITY-1)`, which is the wrap-safe form even though the guard
already proved no wrap occurred. The masking is harmless and defensive, but given the
guard, a plain `+ i * ENTRY_BYTES` would read identically and signal "no wrap is
possible here" to the reader. Minor; the current form is also fine and arguably
more robust if the guard ever loosens.

### S4 — Magic entropy device address `0xD000_3000` is hardcoded in `record_bus`
**File:** `crates/dh-worker/tests/m5_record_replay.rs:148`

`replay_engine.rs:83` has the same literal. `dh_devices::pad::PV_PAD_BASE` is used
symbolically two lines up, but the entropy base is a bare `0xD000_3000`. If
`dh_devices` exports a `PV_ENTROPY_BASE` (the pad and clock bases are exported), use
it for symmetry and drift-safety; if it does not, that's a `dh_devices` gap worth a
follow-up bead, not a blocker here.

### S5 — The acceptance test's progress `eprintln!` cadence is fine; the smoke has none
**File:** `crates/dh-worker/tests/m5_record_replay.rs:469-471` vs the smoke (408-427)

Non-issue, noting for completeness: the smoke (2s debug) needs no progress output;
the acceptance prints every 10 replays under `--nocapture`. Both are appropriate. No
change requested.
