# Action Items

Verdict: **APPROVE**. No action item blocks merging bead 68l. The two Important
items are documentation/tracking follow-ups that must land on their owning beads
(6yl, mmv) so deferred decisions are not lost.

## Critical

- [ ] None.

## Important

- [ ] **I-1 — Make the ENTR device/section conflict explicit and pin it to bead
  6yl.** The entropy device's `DetDevice::snapshot`
  (`crates/dh-devices/src/entropy.rs:171-175`, 16 bytes `{buf_gpa,len,status}`,
  `SECTION_LEN=16`) does NOT match `EntrSection` (the 56-byte ChaCha20 PRNG
  state), yet `tag_for_device_id(0x0004) → ENTR` (`dhsnap.rs:76`) maps the device
  to that tag. If the engine ever auto-routes the device's snapshot bytes to
  ENTR, `EntrSection::decode` will reject them with `BadLength{found:16}`.
  Action: (a) expand the `0x0004` map comment in `dhsnap.rs:76` to state ENTR
  must be sourced from `ctx.entropy.state()` (`EntropyState`), not from
  `PvEntropy::snapshot`; (b) record on bead 6yl's description: "ENTR body =
  56-byte `EntropyState`, NOT the 16-byte device MMIO regs." Out of scope for
  68l's codec, but must not be silently dropped.

- [ ] **I-2 — Record that pv-net's `DEVICE_ID_PV_NET` must equal `0x0007`.** The
  map asserts `0x0007 → NETL` (`dhsnap.rs:79`) for a device that doesn't exist
  yet (lands with bead mmv; no `0x0007` constant in-tree today). The mapping is
  safe now, but when mmv lands the device id must be exactly `0x0007` or it
  diverges from this map. Action: add a note to bead mmv ("pv-net
  `DEVICE_ID_PV_NET` MUST be `0x0007` to match `dhsnap::tag_for_device_id`"),
  and when mmv lands, add a test importing the real constant that asserts
  `tag_for_device_id(DEVICE_ID_PV_NET) == Some(tag::NETL)` (today's test
  hardcodes `0x0007` and won't catch a future constant drift).

## Suggestions

- [ ] **S-1** — Strengthen `device_id_tag_map_is_total_over_known_devices`
  (`tests:108-117`) to assert against the real `DEVICE_ID_*` constants from
  dh-devices/dh-inputlog rather than literals, so the test catches a renumbered
  constant. If a dev-dep is undesirable, at least name the source-of-truth file
  per id in a comment.
- [ ] **S-2** — Add a one-line comment at `dhsnap.rs:195` explaining the
  major-only version gate accepts all additive 1.x minors.
- [ ] **S-3** — Note at `dhsnap.rs:86`/`:123` that `SectionTooLong` is only
  reachable for >4 GiB contents and is intentionally untested.
- [ ] **S-4** — (Optional, no change now) `Container` could be offset-only /
  zero-alloc with a seen-tags bitset if section counts ever grew; current
  `Vec<Section>` + O(n²) dup check is fine for n ≤ 11.
- [ ] **S-5** — (Optional) Add a second byte-level golden with a non-8-aligned
  body (e.g. 12-byte CLKD, pad 4) to freeze alignment-pad placement in a golden,
  complementing `alignment_padding_is_emitted_and_zeroed`.
