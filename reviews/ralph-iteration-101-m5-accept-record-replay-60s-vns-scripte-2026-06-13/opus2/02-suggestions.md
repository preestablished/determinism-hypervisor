# Suggestions (non-blocking)

## S1 — `assert_table_eras` has a small false-PASS window when a script value collides with the previous era

**File:** `crates/dh-worker/tests/m5_record_replay.rs:369-403`

The guest (pad_echo.asm) polls PAD0 once per frame and records `(frame, pad0)`; the latch changes
only at PAD_SET landings. `assert_table_eras` consecutive-dedups the per-frame `pad0` column and
compares it against `expected = [0, script...]` *also* consecutive-dedup'd (line 399-401).

Because **both** sides dedup, the assertion can never produce a false *failure* from a value
collision — that part is sound. But the dedup means a latch era whose value equals the *previous*
era's value (the boot `0`, or the immediately preceding scripted pad) is **invisible** to this
check: a PAD_SET that landed but happened to carry the same `u32` as the value already latched would
not appear as a new era, and the test would still pass. So `assert_table_eras` proves "the eras the
guest observed, *after collapsing repeats*, are a prefix-consistent projection of the script" — it
does **not** prove "every scripted PAD_SET produced a distinct observable era." The module/doc
comment ("guest-observed latch eras == the script") slightly oversells what the dedup'd compare
establishes.

For a fixed `SCRIPT_SEED` SplitMix64 stream of ≤59 `u32` values the collision probability is
negligible (~59²/2³³), and the recorded-vs-script PAD_SET check on line 310 (`pads == script`,
*not* dedup'd) already proves every distinct PAD_SET was logged. So this is genuinely minor. Still,
two cheap hardening options:

- Assert *before* dedup that the number of era *transitions* equals the number of distinct
  consecutive script values — or simply assert the count of recorded PAD_SETs the guest should have
  seen. The line-310 check already does the strong version on the host side, so consider softening
  the doc comment on line 369/402 to say "latch eras (consecutive-deduped) track the script" rather
  than "==".

```rust
// pad0 eras here are consecutive-deduped: a scripted pad whose value equals the
// prior latch is invisible to THIS check. The strong "every PAD_SET landed" proof
// is the host-side `pads == script` compare in record(); this is the guest-visible
// corroboration, not the primary gate.
```

## S2 — The non-1:1 `end_vns` check is meaningful on the RECORD side but near-tautological on the REPLAY side

**File:** `crates/dh-worker/tests/m5_record_replay.rs:284-289` (record) and the engine's
`end_vns` check at `crates/dh-worker/src/replay_engine.rs:327`

`vns` is a pure function `vns_from_icount(icount) = icount * num / den` (vt.rs:43). On the **record**
side, `assert_eq!(last.vns, seconds * VNS_PER_SECOND)` (line 285) is a genuinely valuable check: with
`num=10_000` it would catch a clock-config wiring bug, a vns/icount transposition, or a u128/u64
arithmetic error that a 1:1 ratio would mask (10_000×100_000×seconds must equal seconds×1e9). Good.

On the **replay** side, however, the engine's `out.vns != header.end_vns` check (replay_engine.rs:327)
cannot fail *independently* of the `end_icount` check once the clock ratio is header-verified
(replay_engine.rs:110-114) and `end_icount` is separately asserted equal (line 360 here): both
`out.vns` and `header.end_vns` are `vns_from_icount(end_icount)` under the same ratio, so equal
icount ⇒ equal vns deterministically. The module doc (lines 9-13) frames the non-1:1 clock as
"the first gate to exercise the replay engine's `end_vns` check unmasked" — that's true in the sense
that the *value* is now distinct from the icount axis (so a hypothetical bug that returned `icount`
instead of `vns` would be caught), but it is not true that this run can make the replay-side `end_vns`
compare fail while `end_icount` passes. Worth a one-line doc tightening so a future reader doesn't
over-trust the replay-side `end_vns` as an independent oracle. No code change.

## S3 — `gettid()` `unsafe` block is duplicated verbatim from the 39w template

**File:** `crates/dh-worker/tests/m5_record_replay.rs:88-94`

Identical to `replay_engine.rs:41-47` and `recording.rs:551-557`. The SAFETY comment ("argless
syscall") correctly discharges the obligation, so this is purely a DRY note. A shared
`tests/common/mod.rs` helper (`pub fn gettid() -> i32`) would remove three copies and is already the
home for the other shared rig. Non-blocking; the duplication is trivially correct.

## S4 — `replay_once` returns the slot only so callers can read the guest table; the coupling is implicit

**File:** `crates/dh-worker/tests/m5_record_replay.rs:326-364, 462-468`

`replay_once` returns `SlotVm` purely so the x100 loop can call `assert_table_eras(&slot, …)` on
legs 1 and last. That's fine, but the contract ("this returns a slot whose guest RAM still holds the
replay's table") is implicit. A one-line doc on `replay_once`'s return ("returns the post-replay slot
so the caller can inspect the guest observation table") would make the intent obvious and stop a
future cleanup from changing the return type to `()`. Non-blocking.
