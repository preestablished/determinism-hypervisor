# Positive Notes

### Total decoder, proven total — not just claimed.

`Container::parse` (dhsnap.rs:187-254) is a genuine total decoder: I threw 14
adversarial byte patterns at it (forged 4 GiB−1 len, pad bytes cut at EOF,
exact-fit boundaries, zero-length sections, `count = u32::MAX`, trailing slop) and
every one returned `Ok`/`Err` with no panic and no allocation blow-up. The two
totality smokes (`arbitrary_truncations_never_panic` :314, `single_byte_
corruptions_never_panic` :322) are exactly the right tests for untrusted-input
parsing and mirror the DHILOG battery's rigor.

### Overflow reasoning is correct AND defended with a comment.

dhsnap.rs:217-225: `contents_end = SECTION_HEADER_LEN.checked_add(len)`, then
`padded_end = contents_end.checked_add(pad)`, each `.ok_or(Truncated)`. The comment
("on 32-bit hosts checked_add keeps the decoder total anyway") shows the author
thought about the 32-bit case, not just the 64-bit happy path. The separate
`bytes.len() - offset < padded_end` bound (:226) means `offset += padded_end`
(:243) can never push `offset` past `bytes.len()` — the increment needs no
`checked_add` and correctly doesn't have one. This is precisely the kind of
"prove the boundary, then the arithmetic is free" reasoning that's easy to get
subtly wrong.

### Reserved-means-zero enforced in all three places.

Header `_pad` [12..16) (:199), section-header `_pad` [6..8) (:213), and the
inter-section alignment padding (:235) are all checked nonzero → reject. This is
the determinism-critical rule (any nonzero reserved byte would change the snapshot
ref while meaning nothing), and it's enforced uniformly, with a negative test for
each (`rejects_nonzero_header_pad`, `rejects_nonzero_section_pads`). Matches the
DHILOG `ReservedNonzero` discipline.

### Convention parity with the DHILOG reader (iteration 61) is close and deliberate.

Same `ReadError` enum shape and variant naming
(`TooShort`/`BadMagic`/`UnsupportedVersion { found }`/`Truncated { index }`/
`NonzeroPadding`/`SectionCountMismatch { header, actual }`), same "major byte must
be 1, minors additive" version rule (:195), same `try_into().unwrap()` on
provably-sized slices, same up-front full-validation-then-infallible-accessors
contract. A reviewer who knows the DHILOG reader can read this file with zero
surprises — that consistency is worth a lot across a determinism platform.

### Single source of truth for the device-id↔tag map, honored.

The `DetDevice` trait doc (`crates/dh-devices/src/lib.rs:40-42`) demands the
id↔tag mapping live "in dh-snapshot, one place." `tag_for_device_id` (dhsnap.rs:71)
is that one place, with each arm annotated by device and bead. The
`device_id_tag_map_is_total_over_known_devices` test pins it including the
`None` boundary at 0x0008.

### Writer and reader symmetric on the rules that matter.

Both enforce known-tags-only and unique-tags (`WriteError`/`ReadError` mirror each
other). The writer can't emit a container the reader would reject on those axes —
a nice invariant, and `deterministic_byte_identical_rebuild` (:95) pins the
byte-determinism property the whole format exists to provide.

### Typed-section field-offset pins catch wrong-but-symmetric layouts.

`typed_sections_roundtrip_and_pin_their_layout` (:82-91) asserts the exact LE byte
offsets of every `TimeSection`/`EntrSection` field, so a swapped or wrong-width
field fails even though a pure round-trip would pass. This is the right instinct
for layout that feeds a hash, and it correctly notes `word_pos` is `u128`
(16 bytes at [40..56)) — an easy field to under-size.
