# Suggestions (non-blocking)

### S-1. Golden-bytes division of labor with bead 9tl: this bead's inline golden does NOT match DHILOG's fixture discipline — make 9tl the real freeze.

`tests/dhsnap_codec.rs:121-154` (`golden_bytes_minimal_container`) hand-assembles
the expected bytes in-test and asserts `w.finish() == expect`. That is a good
*writer-layout* pin, but it is **not** a golden freeze in the DHILOG sense.
Compare `crates/dh-inputlog/tests/golden.rs` (bead bp9): it checks in real binary
fixtures (`tests/fixtures/v1_kitchen_sink.dhilog`, `v1_minimal.dhilog`) and pins
their **BLAKE3** with an explicit anti-laundering rule ("any change that touches
BOTH these constants and the fixture files in one PR is laundering a format
break — reject"). The DHSNAP inline test has no such property: if the writer and
the in-test `expect` builder drift *together* (e.g. someone "fixes" both to a new
layout), the test stays green and the format silently changes. The whole point of
a checked-in + hash-pinned fixture is that the bytes live outside the code that
produces them.

Recommended division of labor:
- **68l (this bead):** keep `golden_bytes_minimal_container` as a *layout-offset*
  assertion — it's a useful, readable description of the wire format and the LE
  field offsets (the `typed_sections_roundtrip_and_pin_their_layout` test at
  :82-91 is the same spirit and worth keeping).
- **9tl (P0, now unblocked):** owns the actual freeze — checked-in `.dhsnap`
  fixtures (a minimal one and a kitchen-sink one covering all 11 tags + both typed
  sections) with BLAKE3 pins and the same "never regenerate + re-pin in one PR"
  discipline DHILOG uses. That is what the §4 table line "golden tests" is
  promising, and round-trip/inline tests cannot deliver it.

Flag in 9tl's description that the `full_container()` helper here is a ready-made
kitchen-sink generator.

### S-2. `Truncated { index }` is the *next* section's ordinal on trailing-garbage, slightly misleading.

Probe 6 (trailing 3 bytes after a valid section, fewer than `SECTION_HEADER_LEN`)
returns `Truncated { index: 1 }` — but there is no section 1; the garbage is
trailing slop after section 0. The DHILOG reader has the same shape
(`seq_for_err`), so this matches precedent and is harmless for a total decoder.
If forensic precision ever matters, a distinct `TrailingBytes { offset }` variant
would name it better. Non-blocking; only worth it if divergence tooling reads
these variants.

### S-3. `KNOWN_TAGS.contains` and the duplicate scan are O(n) / O(n²) over a fixed 11 — fine, but a sorted-array or bitset would let the compiler check completeness.

`push_section` (dhsnap.rs:117) and `parse` (dhsnap.rs:229) both do
`KNOWN_TAGS.contains(&tag)`; duplicate detection is a linear scan of `seen` /
`sections`. With ≤ 11 sections this is trivially fine (verified no DoS). Mentioning
only because there is no compile-time link between the `tag` module constants, the
`KNOWN_TAGS` array, and `tag_for_device_id` — if a 12th tag is added to one and not
the others, nothing catches it. A small test that asserts `KNOWN_TAGS.len() == 11`
and that every `tag_for_device_id` result is in `KNOWN_TAGS` would close that gap.
(`device_id_tag_map_is_total_over_known_devices` at :108 checks the map values but
not that they're all in `KNOWN_TAGS`.)

### S-4. Writer does not enforce canonical section order — engine-fixed order + doc is the right call, but say so where the writer lives.

The module doc (dhsnap.rs:3-5, 52-54) correctly states the order is caller-fixed
and the codec does not enforce it, because the snapshot engine fixes a single
order for byte determinism. I agree with that position: forcing the writer to sort
into `KNOWN_TAGS` order would be *more* robust (two engines pushing in different
orders would then produce identical refs for identical state — the exact failure
mode the snapshot ref is sensitive to), but it would also silently reorder a
caller that has a deliberate reason, and v1 has exactly one engine. **Take the
position: engine-fixed order is acceptable for v1, but the writer is one
`self.seen`-based assertion away from enforcing canonical order for free**, and
since the determinism cost of getting it wrong is a silently-diverging snapshot
ref, I'd lean toward having `finish()` (or a `finish_canonical()`) assert that
`self.seen` is a subsequence of `KNOWN_TAGS`. At minimum, the "the snapshot engine
fixes it" guarantee should be a testable invariant somewhere in the engine bead,
not just prose here. Document the load-bearing assumption at the `push_section`
docstring, not only the module header.

### S-5. `Container` holds an owned `Vec<Section>` while `parse` could be allocation-free like DHILOG's iterator.

DHILOG's reader (iteration 61) is explicitly "allocation-free over borrowed
payloads" (reader.rs:18-19) — validation walks the bytes and the iterators are
zero-copy views. `Container::parse` instead builds a `Vec<Section>` (dhsnap.rs:203,
238). For ≤ 11 sections this is negligible, and the eager Vec makes `get`/`has`/
duplicate-detection simpler, so this is a reasonable trade. Noting it only as a
consistency observation against the sibling precedent — not worth changing unless
`no_std`/alloc-free parity with DHILOG becomes a stated goal for this crate.
