# Critical & Important findings

## Critical

None.

---

## Important

### I1 — The u8-fit guarantee silently weakens for any future proto variant numbered ≥256

**File:** `crates/dh-inputlog/tests/stop_reason_mirror.rs:13-31`
(`every_proto_stop_reason_fits_the_u8_slot`)

The test scans `for raw in 0..=255i32`, accepts each `StopReason::try_from(raw)` that succeeds,
asserts the result fits a `u8`, and counts the successes, then pins `assert_eq!(seen, 8, …)`.

I verified empirically (prost 0.13.5, throwaway test reverted) that prost's generated
`TryFrom<i32>` is a **closed** conversion: `StopReason::try_from(8)` errors and
`StopReason::try_from(300)` errors *today*. The implication is the problem, not the safety:

- Proto3 enum values are unconstrained `i32`. A future schema could legitimately add, say,
  `SOME_REASON = 300` (or `= 256`). prost would then make `StopReason::try_from(300)` **succeed**.
- The new variant does **not** fit a `u8` — `u8::try_from(300i32)` is `Err`. That is exactly the
  failure this test exists to catch, and exactly the case the DHILOG END `stop_reason` u8 carrier
  cannot represent.
- But the `0..=255` loop **never evaluates `raw = 300`**, so the `u8::try_from` assertion is never
  exercised for it. The mirror claim ("every proto value fits the u8 slot") silently becomes false
  while the test stays green.

The `count == 8` pin does **not** rescue this. A 9th variant placed at 300 leaves the
in-range-scanned count at 8 (the loop still only sees 0..=7), so `seen == 8` holds and the test
passes — the count pin catches *new low-numbered* variants but is blind to exactly the variants
that would break u8-fit. The two pins overlap on the easy case and both miss the dangerous one.

**Why this matters beyond pedantry:** this is the *only* test asserting the u8↔proto fit, and the
codec deliberately does not range-check the byte (reader.rs comment at line ~460). If proto grows a
≥256 value, the wire could carry a `stop_reason` that no `u8` END slot can encode, and nothing in
the workspace would flag it until a real run truncated/aliased it.

**Honest fix (no perfect option — prost gives us no value list):**
prost 0.13 does **not** generate a `values()`/`VARIANTS` const for enums (confirmed: only
`as_str_name` / `from_str_name` exist in the codegen), so iterating the true value space isn't
free. Two acceptable fixes:

1. **Pin the max wire number, not the count.** Add an explicit upper-bound assertion that the
   highest-numbered known variant fits `u8`, derived from the *known maximum* rather than a scan —
   e.g. assert `StopReason::Faulted as i32 == 7` and `u8::try_from(7).is_ok()`, and pin that 7 is
   the max by asserting `StopReason::try_from(8).is_err()` (the first gap above the top). A future
   ≥256 variant moves the "first gap" and breaks this honestly.
2. **Scan a range that brackets the contract.** Replace `0..=255` with `0..=i32::from(u8::MAX) + 1`
   only if you also assert that the *first rejected* value above the known max stays ≤256 — i.e.
   explicitly encode "the proto must not number any variant ≥256" as the testable invariant.

Either way, write the invariant down as a comment: *"proto StopReason must never number a variant
≥256, because the DHILOG END slot is a single u8."* Right now that constraint is real but unstated,
and the test's structure hides its own blind spot.

**Severity rationale:** Important, not Critical, because (a) no current variant triggers it,
(b) adding a ≥256 enum value is an unusual schema move, and (c) the *golden-fixture* test in the
same file still independently pins the two values that exist. But the test's own comment
("every proto StopReason value must fit") overpromises relative to what it checks, which is the
kind of false assurance a pin test is supposed to eliminate.
