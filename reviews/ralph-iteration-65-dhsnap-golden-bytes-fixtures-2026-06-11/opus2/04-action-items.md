# Action Items

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] **(S1) Clarify the VCPU expression's reliance on the hash leg.** In
  `crates/dh-snapshot/tests/golden.rs`, the kitchen-sink VCPU payload
  `(0u8..200).map(|i| i ^ 0xA5)` is duplicated between `build_kitchen_sink()` (~line
  60) and `kitchen_sink_fixture_parses_to_pinned_sections` (~line 138). The shared
  expression means the reader-half assert is NOT an independent pin — only
  `KITCHEN_SINK_BLAKE3` catches an expression typo. Add a one-line comment saying so,
  or hoist the byte-order-sensitive payloads into a shared `const`/helper
  (`fn vcpu_payload() -> Vec<u8> { (0u8..200).map(|i| i ^ 0xA5).collect() }`) so the
  two legs can't drift relative to each other. Clarity only; no behavior change.

- [ ] **(S2) File a follow-up bead for the stale API.md §4 EVTC row.**
  `.agents/docs/determinism-hypervisor/API.md:621` describes `EVTC` as "channel base
  GPA `u64`" (8 bytes), but the real owner is
  `dh-devices::detchannel::EVTC_LEN = 4+4+4+5+5+1+16 = 39` bytes, which this fixture
  correctly freezes. Do NOT change the spec in this bead. Open a separate bead to
  reconcile the §4 table with the 39-byte owner layout.

- [ ] **(S3) Optionally pin total file length in the kitchen-sink parse test.** Add
  `assert_eq!(fixture.len(), 684);` to
  `kitchen_sink_fixture_parses_to_pinned_sections`
  (`crates/dh-snapshot/tests/golden.rs`) to mirror the `len == HEADER_LEN` anchor the
  minimal test already has. Redundant with the hash pin; purely for human
  readability.
