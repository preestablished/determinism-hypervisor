# Action Items

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] **CI guard against silent-freeze-break (S1).** Add a CI check that fails a
  PR which edits both `crates/dh-inputlog/tests/fixtures/v1_*.dhilog` and the
  `KITCHEN_SINK_BLAKE3` / `MINIMAL_BLAKE3` constants in
  `crates/dh-inputlog/tests/golden.rs` without bumping `FORMAT_VERSION` in
  `crates/dh-inputlog/src/dhilog.rs`. (No such guard exists today.) A CODEOWNERS
  entry or required-label gate on those paths is an acceptable lighter
  alternative. Also add a comment beside the hash constants restating the
  "bump FORMAT_VERSION + new fixture, never edit in place" policy.

- [ ] **Pin the typed DEV_EVENT / AUX bodies in the parse test (S2).** In
  `kitchen_sink_fixture_parses_to_expected_structure`
  (`crates/dh-inputlog/tests/golden.rs`), add `match` arms asserting
  `RecordBody::DevEvent` for records 2/3/4 (device_id=`DEVICE_ID_DETCHANNEL`,
  event_type=`EVENT_RING_PUSH`/`EVENT_CONS_BUMP`/`EVENT_PIO_ANSWER`, and the
  decoded ring_id/new_prod/new_cons/port/value subfields) and optionally for the
  ENTROPY/SDK_EVENT/FRAME_MARK records. This makes the golden pin both the
  writer encoding and the reader decoding of every kind, not just the byte image.

- [ ] **Document the deliberate freeze omissions (S3).** Add a sentence to the
  `tests/golden.rs` module doc noting that `KIND_EPOCH_HASH`, `KIND_NET_TX`, the
  `FLAG_EPOCH_HASHES` path, and the unsealed (`SEALED==0`) path are not
  writer-emittable in Phase 1 and are therefore frozen by the 29-test reader
  battery rather than by this fixture — and that M5 should extend the freeze when
  those emitters land.

- [ ] **(Optional) Assert body_hash in the parse test (S4).** Recompute
  `blake3::hash(&fixture[256..])` and assert it equals header bytes
  `[208..240)` to make the seal invariant self-documenting in the golden,
  independent of the reader's internal validation.
