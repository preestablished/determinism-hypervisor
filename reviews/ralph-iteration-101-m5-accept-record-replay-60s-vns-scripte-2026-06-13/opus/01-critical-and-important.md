# Critical and Important Findings

## Critical

**None.** No data-loss, crash, security, or broken-assertion issues. The test
exercises the real `replay_segment` contract (it does not re-implement the hash
chain or epoch verification), the success path and the byte-identity reseal are
asserted, and the boundary arithmetic is correct (verified below).

Boundary arithmetic that I checked and found correct:
- `pad_script(seconds)` yields exactly `seconds - 1` pads (`1..seconds`).
- The record loop applies `script[(i-1)]` only when `i < seconds`, indexing
  `script[0 .. seconds-1]` — no off-by-one, no out-of-bounds.
- `replay_once` asserts `records_applied == seconds - 1` and
  `epoch_hashes_verified == seconds`, both consistent with the recording.
- The non-1:1 clock identity holds: `vns_from_icount(6_000_000)` with `num=10_000`
  `den=1` is `60e9`, matching `60 * VNS_PER_SECOND`. This genuinely exercises the
  `end_vns` divergence path in `replay_engine.rs:327` that prior 1:1 tests masked.

---

## Important

### I1 — `expected.dedup()` is an asymmetric assertion-weakener in `assert_table_eras`
**File:** `crates/dh-worker/tests/m5_record_replay.rs:399-402`

```rust
let mut expected = vec![0u32];
expected.extend(pad_script(seconds));
expected.dedup();
assert_eq!(eras, expected, "guest-observed latch eras == the script");
```

`eras` is built by **consecutive-dedup** of the guest's observed `pad0` column
(line 395-397): each distinct latch *era* the guest actually saw, in order. The
`expected` side then *also* applies `dedup()` to `[0, script...]`. This makes the
assertion tautologically tolerant of one specific real divergence:

If two consecutive scripted pads ever carry the **same value** (`script[k] ==
script[k+1]`), `expected.dedup()` collapses them to one era. But if the guest
genuinely *failed* to observe one of the two eras (e.g. a replay dropped a PAD_SET
application so the latch never changed twice), `eras` would *also* show one era —
and the two sides would still match. The dedup on the expected side erases the
ability to distinguish "two equal scripted values" from "one value the guest only
saw once because an input went missing."

I verified the **current fixed seed** (`0x00A5_E060_5EED`) produces no consecutive
duplicates and no zero values at either `seconds=6` or `seconds=60`, so today
`expected.dedup()` is a no-op and the assertion is exact. The hazard is latent: the
module comment at line 81 explicitly invites changing the seed ("any fixed value"),
and a future seed with one adjacent collision would silently weaken this guest-side
check — in a determinism-critical gate, exactly the kind of erosion that lets a real
divergence pass.

**Severity rationale:** Important rather than Critical because (a) the host-side
reseal-hammer in `replay_segment` still covers the affected leg byte-for-byte, so a
dropped PAD_SET would be caught there regardless, and (b) the current seed makes it
inert. But the prompt's guidance — "a tautological check that could let a real
divergence pass IS Important" — fits precisely.

**Suggested fix (cheapest): assert the seed has no adjacent collisions, so the
`dedup()` no-op is a checked invariant rather than an accident:**
```rust
let script = pad_script(seconds);
debug_assert!(
    script.windows(2).all(|w| w[0] != w[1]) && !script.contains(&0),
    "fixed seed must yield no adjacent-equal pads and no 0 \
     (else expected.dedup() would mask a missed guest era)"
);
let mut expected = vec![0u32];
expected.extend(script);
expected.dedup(); // now a checked no-op for this seed
```

**Or (stronger): drop the `dedup()` on the expected side entirely and build the
expectation as the exact era sequence the guest must show**, so the two sides are
compared without any collapsing on the expected half. With the current seed this is
behavior-identical and removes the masking entirely:
```rust
let mut expected = vec![0u32];
expected.extend(pad_script(seconds));
// no dedup: the fixed seed yields distinct adjacent eras, and a guest that
// skips one would now mismatch.
assert_eq!(eras, expected, "guest-observed latch eras == the script");
```
(If you take this route, keep the seed invariant as a `debug_assert!` so a future
seed change fails loudly at the test rather than silently changing semantics.)
