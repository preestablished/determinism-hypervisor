# Critical & Important

## Critical

None. The change is correct: the writer guard, reader validation, test flips, spec amendment,
and ledger entry are mutually consistent, and the chosen direction matches the device's existing
`len == 0` rejection. No replay/splice path regresses (all 50 `dh-inputlog` tests pass;
`dh-devices` builds clean).

## Important

### I1 — Stale comment in `net.rs` now contradicts the landed invariant

**File:** `crates/dh-devices/src/net.rs:154-157` (inside `PvNet::apply_net_rx`)

**Problem.** The change is the implementation of bead 206, but it leaves untouched a comment that
describes the *pre-206 world* and now asserts the opposite of what the codec does:

```rust
// len == 0 rejected here while the DHILOG codec accepts empty
// NET_RX records — the cross-layer zero-length policy is its own
// bead (filed iteration 85); until it lands, recording never
// produces an empty frame so the asymmetry is unreachable.
if len == 0 || len > MAX_FRAME || len > self.rx_cap {
```

Three false statements as of this commit:
1. "the DHILOG codec accepts empty NET_RX records" — it no longer does (`WriteError::EmptyNetRx`,
   reader `1..=2048`).
2. "the cross-layer zero-length policy is its own bead… until it lands" — bead 206 *is* that bead,
   and this very commit lands it.
3. "the asymmetry is unreachable" — there is no longer any asymmetry to be unreachable; all three
   layers now agree.

This is doubly confusing because the same commit *added* a correct comment 90 lines above (the
`NetRxError` doc, lines 61-64) stating the three layers now agree. The tree therefore contains two
comments about the same invariant that directly contradict each other. A future reader who lands on
the `apply_net_rx` body first will believe the codec still accepts empties — exactly the
misunderstanding bead 206 exists to prevent. The bead's own description named `dh-devices net.rs`
as a touched file precisely so this would be reconciled.

**Fix.** Replace lines 154-157 with a comment that matches reality, e.g.:

```rust
// len == 0 is also forbidden at the codec since bead 206 (writer
// returns WriteError::EmptyNetRx; reader validation requires
// 1..=2048), so a delivered frame is never empty — all three
// layers agree (see the NetRxError doc above).
```

Severity is Important rather than Critical because no behaviour is wrong — the `len == 0` guard
itself is correct and stays. The cost is purely maintainability/correctness-of-record: a landed
policy change that leaves a comment asserting the policy hasn't landed undermines the very
cross-layer-agreement story the commit is selling.
