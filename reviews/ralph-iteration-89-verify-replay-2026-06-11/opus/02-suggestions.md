# Suggestions

### S1 — `VerifyReport` ergonomics for cw2's 1000x harness

**File:** `crates/dh-verify/src/verify.rs:38-71`

cw2 runs VerifyReplay over 1000 children and asserts "1000/1000 VerifyDone with
matching end_state_hash, zero Divergence." Against that loop, the current API is
usable but missing a couple of one-liners that the harness will otherwise reinvent:

- `verified()` already gives the per-child pass/fail. Good.
- But the loop will want to *collect divergences for the failure report* (which
  child diverged, where). `divergence()` returns `Option<&VerifyProgress>`, so the
  caller has to `match` to pull `first_bad_epoch`/`what` out. Consider a small
  accessor or making the harness-facing summary a struct
  (`{ verified: bool, first_bad_epoch: Option<u64>, what: Option<&'static str> }`)
  so cw2's 1000-row table is a `.map()` not a `.match`.
- Consider an `artifact()`/`Display` like `gate.rs::GateReport::artifact()` (gate.rs
  L28) so a failing child produces a diagnosable plain-text line without the caller
  hand-formatting `events`. cw2 explicitly mirrors the gate-report style; matching
  that affordance keeps the two harnesses symmetrical.

Not blocking — the primitives are all here — but adding the summary shape now saves
cw2 from growing its own.

### S2 — `verified()` double-scans; the invariant is also slightly loose

**File:** `crates/dh-verify/src/verify.rs:44-47`

```rust
pub fn verified(&self) -> bool {
    matches!(self.events.last(), Some(VerifyProgress::Done { .. }))
        && self.divergence().is_none()
}
```

`divergence()` walks the whole `events` vec; combined with the `last()` check this is
two passes. Minor. More interesting: the executor guarantees a report ends with
*either* a `Done` *or* a `Divergence`, never both, so the `&& divergence().is_none()`
is belt-and-suspenders that can never trip given how `verify_replay` builds the
report. That is fine as a defensive invariant for hand-constructed reports (the unit
test builds them by hand), but a doc note that "a well-formed report has exactly one
terminal event" would make the contract explicit for cw2/rfv.

### S3 — `epoch_len.max(1)` is dead code given config validation

**File:** `crates/dh-worker/src/verify_replay.rs:77`

`machine_config.epoch_len.max(1)` guards against divide-by-zero, but a zero
`epoch_len` cannot reach this point: `replay_segment` calls `machine_config.config_hash()`
at its header check (replay_engine.rs:104), which goes through `canonical_encode()`
→ `validate()`, and `validate()` rejects `epoch_len == 0` with
`ConfigError::ZeroEpochLen` (config.rs:152). So by the time any `Divergence` is
produced, `epoch_len` is provably non-zero. The `.max(1)` is harmless but dead, and
slightly misleading (it implies zero is reachable here). Either drop it or add a
comment that it is purely defensive against an unvalidated config. (Low priority —
and if I1 is fixed with a `what`-aware mapping, the division may move or vanish for
several arms anyway.)

### S4 — The live test covers only the one divergence kind whose arithmetic is correct

**File:** `crates/dh-worker/tests/replay_engine.rs:359-452` (the new
`verify_replay_reports_done_and_divergence`)

The test is good for what it covers: real KVM, a good recording → `verified()` with
10 `EpochOk` + `Done`, and a poisoned-RAM recording → an *Ok report carrying a
Divergence* (explicitly asserting it is NOT an `Err`, asserting `first_bad_epoch == 1`).
That last assertion is exactly the boundary case the prompt flags
(30_000/30_000 = 1, divergence at the first epoch link), and it is correct.

But poisoned RAM diverges at the **first epoch link** — the *only* `what` for which
`first_bad_epoch = at_icount / epoch_len` is honest (see I1). The test therefore
gives false confidence: it would still pass even with the I1 bug fully present for the
other five divergence kinds. Add coverage (can be host-runnable unit tests on the
*wrapper's mapping function* if you extract it, avoiding KVM) for at least:

- an `end_state_hash` divergence (`at_icount = end_icount`) → assert the reported
  `first_bad_epoch` is honest (e.g. `None`/sentinel, not `total_epochs`);
- a `resealed log bytes` divergence (`at_icount` = byte offset) → assert
  `first_bad_epoch` is not a byte-offset-divided-by-epoch_len garbage value.

Extracting the `ReplayError::Divergence` → `VerifyProgress::Divergence` mapping into a
small pure function would make these testable without hardware and is the natural
home for the I1 fix.

### S5 — Doc-comment overstatement (tie-in to I2)

**File:** `crates/dh-verify/src/verify.rs:11,21`; `crates/dh-worker/src/verify_replay.rs:11`

verify_replay.rs:11 says "the engine's structured report maps 1:1." It does not — it
maps 1:1 only for the epoch-chain divergence; for the END-class and resealed-byte
divergences the mapping reinterprets `at_icount` (see I1). And verify.rs:11/21 claim
proto mirroring that does not hold for `Divergence` (see I2). These comments are the
kind that future readers (and rfv's author) will trust. Tighten them to say what is
actually true: the library model is the bead-1py verdict shape; the engine's
`Divergence` is translated (not 1:1) into it; rfv translates again into the proto.
