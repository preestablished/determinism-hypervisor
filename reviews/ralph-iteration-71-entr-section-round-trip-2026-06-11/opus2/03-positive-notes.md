# Positive Notes

### Genuinely end-to-end integration test, not a unit echo
`tests/entr_roundtrip.rs` drives the LIVE `DetEntropy` device (`from_seed` → `fill` →
`state()`), not a hand-built struct, then runs the whole chain: `EntrSectionV2::from_parts`
→ `ContainerWriter::push_section` → `Container::parse` → `get(tag::ENTR)` → `decode` →
`DetEntropy::restore` → bit-identical continuation. This is exactly the M4 golden property,
and it crosses the real crate boundary (`dh-devices` ↔ `dh-snapshot`) rather than faking it.

### Sub-word-fill coverage is deliberate and correct
The test consumes a 37-byte fill before snapshotting and then probes continuation with a
`[1, 4, 7, 64, 1000, 37]` size sweep — directly exercising the word-granularity invariant
documented in `entropy.rs:88-93`. This is the subtle bug-class (sub-word remainder
buffering) that a naive round-trip test would miss; catching it here is the right instinct.

### v1/v2 coexistence proven loud on misuse
`v1_and_v2_sections_coexist_and_misuse_is_loud` keeps the ORIGINAL landmine asserted: a
bare 16-byte device blob is `BadLength{16}` under both `EntrSection::decode(.,1)` AND
`EntrSectionV2::decode(.,2)`, and `from_parts` rejects a 15-byte reg slice. The iteration-64
trap can't silently reappear.

### dh-devices is a dev-dependency only — no production coupling
The `dh-snapshot/Cargo.toml` addition is under `[dev-dependencies]` with a comment tying it
to the bead. `dh-snapshot`'s production code stays free of `dh-devices`; the device↔section
combine/split seam (`from_parts`/`device_regs`/`prng`) is plain data plumbing that the qmp
engine will own. Correct layering — the snapshot crate doesn't grow a runtime dep on the
device crate just to test the round trip.

### The LANDMINE→RESOLVED comment transition is exemplary
`tag_for_device_id` (dhsnap.rs:75-89) doesn't delete the danger — it rewrites the LANDMINE
into a RESOLVED note that still explains the failure mode (`EntrSection::decode ⇒
BadLength{16}`) and names the chosen v2 layout. Future readers see both the hazard and the
resolution. The map comment `// pv-entropy — sec_version 2, see RESOLVED above` is updated
in lockstep.

### Symmetric, self-documenting API surface
`from_parts` (combine) ↔ `device_regs()`/`prng()` (split), `encode`/`decode`, with `LEN` and
`VERSION` consts. The struct field order in memory matches the on-wire byte order matches the
device's `snapshot()` order — verified consistent against `entropy.rs:168-172`. Easy to audit.

### Bead bookkeeping is accurate
6yl's close reason precisely describes what landed (EntrSectionV2, 72B, from_parts/device_regs
seam, v1 retained, M4 proof), and the qmp bead's dependency on 6yl is now satisfied. The
in-tree state matches the tracker.
