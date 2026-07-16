# Suggestions (non-blocking)

## S1 — Add a CI grep guard against "regen + bump hash" in one PR

The freeze is airtight against accidental drift, but the one residual attack is a
single PR that both rewrites the `*.dhilog` binaries and edits the
`KITCHEN_SINK_BLAKE3` / `MINIMAL_BLAKE3` constants. Nothing structural makes that
harder today — there is **no CI guard** (confirmed: no reference to
`DHILOG_REGEN_GOLDEN` or `golden` in `.github/`, `Makefile`, `justfile`, or any
CI config).

Cheap hardening: a CI check that fails if a diff touches both
`crates/dh-inputlog/tests/fixtures/v1_*.dhilog` and the hash constants in
`tests/golden.rs` without a corresponding `FORMAT_VERSION` bump. Even simpler: a
CODEOWNERS entry or a required-label gate on those paths. A code comment near the
constants pointing at the policy ("editing this constant alongside the fixture =
breaking the v1.0 freeze; bump FORMAT_VERSION and add a new fixture instead")
would also raise the friction for a careless reviewer.

## S2 — Parse test skips the typed DEV_EVENT / Entropy / SdkEvent / FrameMark bodies

`kitchen_sink_fixture_parses_to_expected_structure` spot-pins the typed `body()`
for `PadSet` (rec 1), `NetRx` (rec 5), and `TimerFire` (rec 7), but the three
DEV_EVENT records (RING_PUSH rec 2, CONS_BUMP rec 3, PIO_ANSWER rec 4) and the
ENTROPY/SDK_EVENT/FRAME_MARK AUX records are only asserted at the `kind()` level,
not decoded through `RecordBody::DevEvent { device_id, event_type, data }` etc.

The byte-level freeze (hash pin + re-serialization) DOES cover those payload
bytes, so the format itself is frozen — this is not a freeze gap. But the
structural layer is what would catch a *reader* regression that silently
misdecodes a DEV_EVENT subfield while the bytes stay identical. Since the reader
exposes `RecordBody::DevEvent` (`reader.rs:168-172`), adding three more `match`
arms (assert device_id=0x0001, event_type=RING_PUSH/CONS_BUMP/PIO_ANSWER, and the
ring_id/new_prod/new_cons/port/value subfields) would make the golden the
single fixture that pins both the writer encoding and the reader decoding of
every kind. Low effort, closes the loop on the detchannel encodings the bead set
out to cover.

## S3 — Document why EPOCH_HASH / NET_TX / unsealed / EPOCH_HASHES are absent

The review brief asks whether it's OK that `FLAG_EPOCH_HASHES`, `KIND_EPOCH_HASH`
(0x42), `KIND_NET_TX` (0x44), and the unsealed path are absent from the freeze.
**It is OK** — the writer has no method to emit EPOCH_HASH or NET_TX (they're
M5-emission), and `seal()` always produces a SEALED log, so a writer-generated
golden structurally cannot contain them. The reader's acceptance of those kinds
is covered by the 29-test reader battery (which has `KIND_EPOCH_HASH` /
`KIND_NET_TX` arms in `body()`), not by this golden.

The module doc says "NET_TX/EPOCH_HASH emission is M5" but doesn't spell out that
this is *why* they're absent from the v1.0 freeze, nor that the unsealed/
EPOCH_HASHES-flag paths are deliberately out of scope here. One sentence in
`golden.rs`'s doc — "EPOCH_HASH/NET_TX and the unsealed path are not
writer-emittable in Phase 1, so they are frozen by the reader battery, not this
fixture" — would pre-empt the exact question a future reader will ask. When M5
adds those emitters, the freeze should grow a `v1_epoch_hashes.dhilog` (or extend
kitchen-sink under a new hash) to cover them.

## S4 — Consider pinning the body_hash slot explicitly in the parse test

The parse test asserts header fields but does not independently recompute and
assert `body_hash` (it trusts that `LogReader::parse` validated it — which it
does, per the reader battery). Not necessary given the hash-pin on the whole
file, but an explicit `assert_eq!(recomputed_body_hash, header bytes [208..240])`
in the parse test would make the golden self-documenting about the seal
invariant. Purely optional.
