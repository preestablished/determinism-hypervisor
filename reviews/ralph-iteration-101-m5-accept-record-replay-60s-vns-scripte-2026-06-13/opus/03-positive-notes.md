# Positive Notes — patterns worth preserving

### P1 — The non-1:1 clock is genuinely load-bearing, not decorative
**File:** `crates/dh-worker/tests/m5_record_replay.rs:73-74, 138, 284-289`

The 10_000:1 ratio is chosen so `100k icount = 1e9 vns = one guest-second`, and the
test asserts `last.vns == seconds * VNS_PER_SECOND` exactly (line 285-289). This is
the first gate to drive `replay_engine.rs:327`'s `end_vns` check with a value the
icount axis does not mask — the exact path the production code flagged as "masked by
1:1 clocks until a5e". The choice of `CLOCK_NUM` is documented, the identity is
asserted on the record side, and `replay_segment` re-checks it on every replay leg.
This is the highest-value property in the change and it is set up correctly.

### P2 — The guest-RAM era check is independent of the host-side reseal hammer
**File:** `crates/dh-worker/tests/m5_record_replay.rs:369-403`

`replay_segment`'s reseal verifies the **host-side log bytes**; `assert_table_eras`
reads the **guest's own observation table** out of guest RAM and reconstructs the
latch eras the guest actually polled. These are independent witnesses: the reseal
could pass while a PAD_SET never reached the guest's poll loop, and only the table
check would catch that. This avoids the tautology risk the research notes warn about
— the test verifies the contract from two sides, not the same side twice. (See I1 for
the one way the *expected* side currently softens this.)

### P3 — Self-parsing the recorded log pins the rig before any replay runs
**File:** `crates/dh-worker/tests/m5_record_replay.rs:296-312`

Before replaying, `record` re-parses its own sealed log and asserts: the epoch-hash
flag is set, exactly `seconds` EPOCH_HASH records landed, the recorded PAD_SET
sequence `== script` exactly, and the END hash matches the live `end_state_hash`.
This fails the recording rig loudly and locally if the *recording* side is wrong,
so a replay failure can't be misattributed. Good separation of "is the recording
sound" from "does replay reproduce it."

### P4 — Wrapped-ring guard makes the era reconstruction honest
**File:** `crates/dh-worker/tests/m5_record_replay.rs:376-379`

`assert_table_eras` refuses (rather than silently truncating) if the guest's
monotone frame count reached the ring capacity — "era reconstruction would be lossy."
Without this, a 60-second run that overflowed the table would silently compare a
truncated era list and could pass on a partial sequence. Catching the lossy case
explicitly is exactly right for a determinism gate.

### P5 — `#[ignore]` discipline mirrors the M4 perf-gate precedent, with the smoke as the always-on guard
**File:** `crates/dh-worker/tests/m5_record_replay.rs:36, 405-435`

The 11-minute x100 acceptance is `#[ignore]`d with the exact invocation in the
attribute string, and the unignored 6s x1 smoke keeps the *same rig* — same scripted
source, same non-1:1 clock, same `replay_once` verification — in every sweep. This is
the right shape: the expensive proof runs on demand on a quiesced box, while the
cheap version keeps the non-1:1 `end_vns` path continuously covered so it can't silently
rot between acceptance runs.

### P6 — Thorough, accurate module documentation
**File:** `crates/dh-worker/tests/m5_record_replay.rs:1-38`

The header doc explains the clock choice, the grid==quantum==epoch_len decision, the
ARCH §1 placement rationale (why this lives in `dh-worker/tests` and not
`tests/determinism` despite the bead text), and the `#[ignore]` precedent. It states
*why* each non-obvious choice was made, which is exactly where comments earn their
keep. The "PLACEMENT" paragraph in particular preempts the obvious "why isn't this in
tests/determinism" review question with the normative rule and its enforcing test.

### P7 — Fresh slot per replay leg with a documented rationale
**File:** `crates/dh-worker/tests/m5_record_replay.rs:458-462`

Each of the 100 replays builds a fresh `SlotVm` (the restore engine's precondition)
and drops it at scope end, with a comment explaining why ("100 concurrent 16 MiB
guests would be pointless load"). The counter is reset by each restore, and the
per-thread counter is opened once outside the loop. The lifecycle is correct and the
reasoning is captured.
