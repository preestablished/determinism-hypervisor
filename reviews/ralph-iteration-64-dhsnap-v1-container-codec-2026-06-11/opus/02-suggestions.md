# Suggestions (non-blocking)

### S-1 — Device-id↔tag test hardcodes ids; loses its cross-check value as constants drift

**File:** `crates/dh-snapshot/tests/dhsnap_codec.rs:108-117`

`device_id_tag_map_is_total_over_known_devices` asserts against literal
`0x0001..0x0007`. That pins the *map*, but it does not pin the map against the
**authoritative `DEVICE_ID_*` constants** that live in dh-devices/dh-inputlog.
If someone renumbered `DEVICE_ID_PV_CLOCK`, this test would still pass while the
system silently broke. Since dh-snapshot already takes `snapstore-client` as a
dev-dependency, consider importing the real constants in the test (or a small
shared module) and asserting `tag_for_device_id(DEVICE_ID_PV_CLOCK) ==
Some(tag::CLKD)`, etc. This turns the test from "the map is self-consistent"
into "the map agrees with the devices" — the property that actually matters.
(If a dev-dep on dh-devices is undesirable for layering reasons, at minimum add
a comment naming the source-of-truth file for each id.)

### S-2 — Consider documenting why `version >> 8 != 0x01` accepts all minors

**File:** `crates/dh-snapshot/src/dhsnap.rs:195`

The major-only version gate (`version >> 8 != 0x01`) is correct and the test
`b[6] = 0x01 // 1.1 accepted` exercises it, but the rationale ("minor is
additive; a v1 reader tolerates any 1.x") lives only in the `ReadError`
doc-comment at `:155`. A one-line comment at the check site would save the next
reader a hop. Minor.

### S-3 — `SectionTooLong` is structurally unreachable in practice — note it

**File:** `crates/dh-snapshot/src/dhsnap.rs:86-87`, `:123-126`

`WriteError::SectionTooLong` can only fire when `contents.len() > u32::MAX`
(4 GiB), which is untestable at any sane scale — correctly left untested. Worth
a `// (only reachable for >4 GiB contents; untestable)` note so a future
coverage audit doesn't flag it as a gap or try to add a 4 GiB allocation.

### S-4 — `Container` stores an owned `Vec<Section>`; an offset-only design could be zero-alloc

**File:** `crates/dh-snapshot/src/dhsnap.rs:182-184`

`parse` allocates a `Vec<Section>` (borrowing into the input) and the
duplicate-tag check is O(n²) over it. With n ≤ 11 this is completely fine and
the current design is the most readable. Mentioning only for completeness: if
DHSNAP ever grew to many sections, a bitset-of-seen-tags + lazy iteration over
offsets would drop the allocation and the quadratic. Not worth changing now.

### S-5 — Golden fixture covers only the no-pad (56-byte) case at the byte level

**File:** `crates/dh-snapshot/tests/dhsnap_codec.rs:121-154`

`golden_bytes_minimal_container` pins a TIME section whose 56-byte body is
already 8-aligned, so the hand-assembled golden never exercises alignment-pad
bytes *in the golden*. `alignment_padding_is_emitted_and_zeroed` (`:157`) does
check padding via length+suffix assertions, so the behavior is covered — but a
second small golden with, say, a 12-byte CLKD body (pad 4) would freeze the
exact pad-byte placement at the byte level too, matching the "DHILOG fixture
discipline" the module aims for. Optional hardening.
