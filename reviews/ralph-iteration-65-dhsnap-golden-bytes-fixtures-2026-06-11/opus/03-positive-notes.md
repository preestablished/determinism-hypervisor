# Positive Notes

## Faithful, disciplined reuse of the DHILOG freeze pattern
`crates/dh-snapshot/tests/golden.rs` mirrors `crates/dh-inputlog/tests/golden.rs`
leg-for-leg: hash-pin → writer-reproduction → reader decode, the same `load_or_regen`
helper gated on a `*_REGEN_GOLDEN` env var, the same "never regenerate + re-pin in one PR"
laundering warning addressed to reviewers. The consistency across DHILOG (bp9) and DHSNAP
(9tl) makes both formats reviewable with the same mental model.

## Anti-laundering guardrail called out explicitly
The module doc (`golden.rs:78-82`) and the constant comment (`golden.rs:30-35`) both tell a
reviewer exactly what a format-break-in-disguise looks like ("any change touching BOTH these
constants and the fixture files in one PR is laundering a format break — reject unless it is
an explicit version bump landing NEW fixture file names"). This is the single most valuable
property of the freeze and it is documented in-band where a future reviewer will see it.

## Deliberately byte-order-sensitive test vectors
`build_kitchen_sink` (`golden.rs:96-138`) is engineered to catch offset/endianness drift:
sequential MCFG `1..=64`, XOR'd VCPU `i^0xA5` across a >128-byte payload, and ascending
multi-byte engine constants (`0x0102…0708`, `0x1112…1718`, `0x2122…2728`, and the 16-byte
`word_pos` whose two 64-bit halves carry distinct byte ranges so a half-swap is caught).
This is a notable step above "fill with 0xAA" fixtures.

## Accurate, honest freeze-scope documentation
`golden.rs:67-73` is careful not to overclaim: it states the container framing + engine-owned
TIME/ENTR are frozen here, while device CONTENTS are owner-frozen by each device's
round-trip test. The fixture payloads are labelled framing-representative, not authoritative
device state. This matches the ownership split in `dhsnap.rs:12-21` and iteration 64's
review note, avoiding a false sense that device blobs are pinned here.

## Complete reader-half coverage
`kitchen_sink_fixture_parses_to_pinned_sections` (`golden.rs:188-233`) pins all 11 §4
sections plus the tag ordering against `KNOWN_TAGS`, and uses the typed `TimeSection`/
`EntrSection` decoders for the engine sections — so a reader-side decode regression
(swapped offset, wrong endianness) fails even though the on-disk bytes are unchanged. The
minimal fixture additionally pins the exact header length.

## Minimal-footprint wiring
`*.dhsnap binary` (`.gitattributes`) cleanly follows the `*.dhilog binary` precedent, and
`blake3.workspace = true` reuses the existing workspace dependency rather than introducing
a new version. No incidental churn.
