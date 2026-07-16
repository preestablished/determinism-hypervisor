# Suggestions (non-blocking)

## S1 — Make the VCPU expression's reliance on the hash leg explicit

**File:** `crates/dh-snapshot/tests/golden.rs:60-64` (builder) and `:138-141` (asserter)

The kitchen-sink VCPU payload `(0u8..200).map(|i| i ^ 0xA5)` is written in the
builder and re-derived in `kitchen_sink_fixture_parses_to_pinned_sections`. Because
both legs share the expression, the round-trip assert cannot catch a typo in the
expression — only the literal `KITCHEN_SINK_BLAKE3` hash constant catches that
(the writer's bytes would shift and the fixture would stop matching). That is fine,
but a future reader could mistake the reader-half assert for an independent pin.

Either of these would document the actual invariant:

```rust
// The reader-half asserts below re-derive the same expressions as
// build_kitchen_sink(); they pin the *parse path*, not the bytes. The byte
// pin is KITCHEN_SINK_BLAKE3 — change any expression and the hash assert fires.
```

…or factor the byte-order-sensitive payloads into shared `const`s / a helper so the
builder and asserter cannot drift relative to each other at all:

```rust
fn vcpu_payload() -> Vec<u8> { (0u8..200).map(|i| i ^ 0xA5).collect() }
```

Pure clarity/maintainability; no behavior change.

## S2 — Surface (don't fix here) the stale §4 EVTC spec-table entry

**File:** `.agents/docs/determinism-hypervisor/API.md:621` (out of this bead's scope)

The §4 table describes `EVTC` contents as "channel base GPA `u64`" (8 bytes), but the
real owner is `dh-devices::detchannel::EVTC_LEN = 4+4+4+5+5+1+16 = 39` bytes, and
this fixture correctly freezes 39 bytes. The fixture and the `golden.rs` module-doc
("EVTC at its v1 39-byte length") are *right*; the spec table is the stale leg.

This bead correctly does **not** touch the spec, and its "framing-representative, not
authoritative" disclaimer covers the contents shape. Recommend filing a follow-up
bead to reconcile API.md §4's EVTC row with the 39-byte owner layout so the spec
table stops implying an 8-byte `u64`. Not a blocker for this freeze.

## S3 — Consider asserting `fixture.len()` in the kitchen-sink parse test

**File:** `crates/dh-snapshot/tests/golden.rs:130-167`

`minimal_fixture_parses_empty` pins `fixture.len() == HEADER_LEN`, a nice cheap
total-size anchor. The kitchen-sink parse test pins per-section contents and tag
order but not the total file length (684). The hash already pins length implicitly,
so this is redundant — but a one-line `assert_eq!(fixture.len(), 684)` would make the
overall framing size a human-readable invariant alongside the per-section asserts.
Optional.
