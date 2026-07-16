# Positive Notes

### P1 — The interop test runs the *real* host codec, not a re-implementation

`tests/nanokernel/tests/capture_manifest_interop.rs` rebuilds the channel page byte-for-byte
the way the asm does, then drives the genuine `detguest-host` `Channel::attach`,
`read_manifest`, `resolve`, and `read_region`. This is exactly the right shape for a wire
fixture: it proves the asm's bytes are *decodable by the production reader*, catches
field-offset drift through `RegionEntry::read_from`, and verifies the `read_region` extent
walk reproduces the framebuffer pattern. The third test
(`read_region_walks_the_extent_into_the_known_content`) additionally checks an interior
unaligned slice agrees with the full read and that a read past `region.len` is *refused, not
truncated* (lines 145-153) — that over-read assertion is precisely the kind of total-decoder
property the research context calls for, and it is easy to forget.

### P2 — Drift pins derive from shared constants and `detguest-wire`, not re-typed literals

`capture_fixture_asm_matches_rust_constants` (`tests/nanokernel/tests/elf_shape.rs:330-396`)
ties the asm's manifest constants back to the codec's own truth — `MANIFEST_MAGIC`,
`OFF_MANIFEST`, `RegionEntry::offset(0)`, `Extent::offset(0)`,
`REGION_FLAG_FRAMEBUFFER` — rather than to hand-copied numbers. This is the correct mirror-test
discipline: the asm "cannot drift from the codec" because the assertion *is* the codec. The
comment on lines 379-381 makes that intent explicit.

### P3 — Compile-time guard against a channel/framebuffer overlap

The `const _FB_CLEAR_OF_CHANNEL` assertion (`elf_shape.rs:390-392`) encodes the security-relevant
invariant — a manifest extent overlapping the channel page would let capture reads observe ring
traffic — as a `const` assert that fails the clippy lane, not just a runtime check. Good
defense-in-depth for the C5 capture-neutrality acceptance this fixture underpins.

### P4 — Clean reuse of the canonical channel header and the no-seqlock-needed insight

The fixture reuses the exact canonical header layout from `device_exercise` (same ring descs,
same power-of-two W size with the documented 0x1E0000 caveat) rather than inventing a parallel
layout. The module header's reasoning that generation can stay 0 — "the guest writes the
manifest BEFORE CHANNEL_INIT, so generation stays 0 (even) — no seqlock dance needed: the
host's first reader runs at attach, after everything is in place" — is correct and well
explained, and it keeps the fixture minimal. Leaning on zeroed guest RAM for the 63 unused
slots, name padding, generation, region_id, gva, and extent_off is both correct and concisely
documented (asm lines 143-145).

### P5 — Faithful, well-commented progress-byte / failure-letter protocol

The `FDX` success sequence with lowercase-on-failure parking mirrors the established
`device_exercise` "CEPBDX" convention exactly, and `CAPTURE_FIXTURE_OK_SEQUENCE` is exported
so the (hardware-gated) harness has a single source of truth. The cmdline `layout_version`
parse — leading decimal digits, "0"/empty/no-digits → default — reuses the `landing_loop`
contract and is the deliberate bumpable knob the C2 FAILED_PRECONDITION test will turn; the
`bumped_layout_version_is_visible_to_resolve` test (lines 145-156 of the interop file) proves
that knob is observable through the real resolve path with an explicit `assert_ne!` against
the default.
