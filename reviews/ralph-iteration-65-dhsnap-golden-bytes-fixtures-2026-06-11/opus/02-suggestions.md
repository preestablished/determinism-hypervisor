# Suggestions (Non-Blocking)

All three are optional polish; none gate the merge. The pattern is mature and this is its
third reviewed application — I am deliberately not re-litigating the design.

## S1 — Optionally positively pin the header version/magic in the parse test

`crates/dh-snapshot/tests/golden.rs:188-192` (`minimal_fixture_parses_empty`) asserts
`sections().count() == 0` and `len() == HEADER_LEN`, but neither golden test positively
asserts the on-disk `FORMAT_VERSION` (0x0100) or magic. `Container::parse` rejects a wrong
version, so a regression can't slip through *via the writer*, and the frozen-bytes hash
already pins the literal header bytes. But the DHILOG golden does positively assert
`h.version == FORMAT_VERSION`, and an explicit version assertion documents intent and
guards against a future `Container` that silently accepts a higher minor without surfacing
it. A one-liner reading the version field from `fixture[6..8]` (or exposing it on
`Container`) would close the small parity gap. Low value, purely defensive.

## S2 — `epoch_index = 20` is the one byte-order-weak engine field

`crates/dh-snapshot/tests/golden.rs:64,114-116` — `TimeSection.epoch_index = 20` encodes
to `[20,0,0,0,0,0,0,0]`. Unlike `cumulative_icount`/`vns` (ascending-distinct bytes), this
field's bytes are mostly zero, so it contributes little to catching a within-field byte
permutation of *that specific field* (a full reversal still differs, so endianness is
covered by the sibling u64s; this is only a theoretical within-field-shuffle gap). Since
`icount` and `vns` already exercise the u64 LE layout exhaustively, this is immaterial —
but if a future maintainer wants every engine field independently byte-order-discriminating,
giving `epoch_index` an ascending-distinct value like `0x0708090A0B0C0D0E` (then re-pinning
into NEW fixture files under a version bump, never in place) would make it uniform.

## S3 — Consider asserting `sec_version` on the framing-representative sections

The reader half pins TIME/ENTR `sec_version` implicitly via `decode(..., t.sec_version)`,
but the device-shaped sections (MCFG/VCPU/LAPC/CLKD/PADD/EVTC/BLKO/NETL/SERL) are all
written with `sec_version = 1` and the parse test does not assert that field back. It is
covered by the byte-level hash, so this is only about making the reader-half assertions
self-describing. A single `assert_eq!(c.get(tag::MCFG).unwrap().sec_version, 1)` (or a loop)
would make the per-section version part of the reader freeze explicit. Very low priority.
