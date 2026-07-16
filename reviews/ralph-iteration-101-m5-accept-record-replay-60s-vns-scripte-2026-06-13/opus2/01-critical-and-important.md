# Critical and Important Issues

## Critical

None found. The test exercises the production `replay_segment` contract (epoch-link verification,
`end_vns`, `end_state_hash`, the reseal hammer) rather than re-deriving the chain math itself, so
there is no tautology that could let a real replay divergence pass. The two divergence-detecting
properties that matter most — every `EPOCH_HASH` verified and `end_state_hash` reproduced — live
inside `replay_segment` and are merely *counted/pinned* here, which is the correct division of labor.

## Important

### I1 — The acceptance gate is coupled to the DHILOG *byte layout* via the reseal hammer, not just to determinism

**Severity:** Important (maintainability / spurious-failure risk on a normative acceptance gate)

**File:** `crates/dh-worker/tests/m5_record_replay.rs:362`
(and the property is inherited from `crates/dh-worker/src/replay_engine.rs:376` `resealed != log_bytes`)

```rust
assert_eq!(outcome.resealed, rec.log, "the reseal hammer");
```

`replay_segment` already returns an error if its internal `resealed != log_bytes` check fails
(replay_engine.rs:376), so this assertion is *redundant for catching divergence* — the
`.expect("replay must not diverge")` on line 353 has already fired in that case. What this line
adds is a second, test-level assertion that the resealed bytes equal *this specific recording's*
bytes. That is fine in itself, but combined with the production-side reseal hammer it means the
**M5 acceptance gate now fails on any change to the DHILOG on-disk byte layout** — record framing,
header field offsets, an added AUX record kind, an encoder-fingerprint bump — even when determinism
and replay fidelity are completely intact.

This is the classic "over-assert on incidental details" trap (your research notes call it out). The
*product property* M5 is supposed to gate is "replay reproduces `end_state_hash` + every `EPOCH_HASH`
+ `end_vns` with zero divergence." Byte-identical reseal is a *stronger* implementation property the
replay engine happens to provide; pinning it inside an acceptance gate means a legitimate,
determinism-preserving log-format refactor reddens the M5 gate and forces a re-bless of an 11-minute
×100 run on the lab box.

Note this is partly inherent to `replay_segment`'s own contract (the engine itself enforces byte
identity), so the test cannot fully escape it. But the test should not *additionally* re-pin the
exact bytes at the acceptance layer, and it should make the intent explicit so a future refactorer
knows the reseal compare is the engine's strength, not an M5 requirement.

**Suggested fix:** Drop the redundant test-level byte compare and rely on the engine's own reseal
check + the semantic outcomes; or, if you want a visible reseal assertion, assert only that a reseal
*was produced and is non-empty / has the expected record counts*, leaving the byte-identity to the
engine where it belongs:

```rust
// The engine already enforces byte-identical reseal internally (replay_engine.rs);
// at the acceptance layer pin only the SEMANTIC identity, so a determinism-preserving
// log-layout refactor does not redden the M5 gate.
assert_eq!(outcome.records_applied, seconds - 1, "every scripted pad");
assert_eq!(outcome.epoch_hashes_verified, seconds, "every EPOCH_HASH verified");
assert_eq!(outcome.end_icount, seconds * QUANTUM);
assert_eq!(outcome.end_state_hash, rec.end_state_hash);
// (reseal byte-identity is the engine's own contract, not an M5 acceptance criterion)
```

If the team *deliberately* wants the acceptance gate to also lock the wire format (a defensible
position — it makes format changes a conscious re-bless), then keep the assertion but add a one-line
comment saying so, so the next person who refactors the log layout understands the breakage is
intended and re-blessing is the expected workflow. The blocking ask is the explicit decision +
comment, not necessarily the deletion.
