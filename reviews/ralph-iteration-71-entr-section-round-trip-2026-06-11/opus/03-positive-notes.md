# Positive Notes

### P1 — Layout is byte-exact and matches the device verbatim

`EntrSectionV2::encode` lays out `seed[0..32] ‖ stream[32..40] ‖ word_pos[40..56] ‖
device_regs[56..72]`, and `device_regs()` lays out `buf_gpa[0..8] ‖ len[8..12] ‖
status[12..16]` — all little-endian. That is **identical** to the pv-entropy device's
`DetDevice::snapshot` (`entropy.rs:168-172`) and `restore` (`entropy.rs:174-182`), which
emit/consume `buf_gpa LE ‖ len LE ‖ status LE` with `SECTION_LEN = 16`. The v1 prefix
(`seed ‖ stream ‖ word_pos`) is identical to `EntrSection` (`dhsnap.rs:340-346`), so
`prng()` and a v1 decode see byte-for-byte the same 56-byte head. I checked every offset;
they all line up. This is the crux of the bead and it is correct.

### P2 — Version discipline is loud and complete

`EntrSectionV2::decode` rejects both wrong `sec_version` (`BadVersion`) and wrong length
(`BadLength`), and the test `v1_and_v2_sections_coexist_and_misuse_is_loud` pins all the
right negatives: v1 bytes refuse to decode as v2 (wrong version *and* wrong length), a
bare 16-byte device blob is `BadLength{16}` under *both* versions (the original landmine
stays loud), and `from_parts` rejects a 15-byte blob with `BadLength{15}`. v1 decode is
completely untouched. This is exactly the right failure surface.

### P3 — The golden test genuinely proves the M4 property end-to-end

`restored_prng_reproduces_the_next_draws_bit_identically` drives the *full* chain —
live `DetEntropy` → `state()` → `EntrSectionV2::from_parts` → `encode` → real
`ContainerWriter` → `Container::parse` → `get(ENTR)` → `decode` → `DetEntropy::restore` —
not a shortcut. It burns 1024 + 37 bytes first (the 37-byte fill deliberately exercises
the sub-word/word-granularity invariant from `entropy.rs:87-97`), then asserts
bit-identical continuation across fills of `[1, 4, 7, 64, 1000, 37]` bytes, mixing
sub-word and multi-KB draws. It also asserts the device-reg half survives the trip
(`back.device_regs() == device_regs`). This is a real golden test, not a smoke test.

### P4 — The landmine comment was upgraded honestly

The old `LANDMINE` comment (speculative: "6yl decides where the reg blob lands") is
replaced with a precise `RESOLVED (bead 6yl)` note that states the chosen layout (72 bytes
= PRNG ‖ regs), how it is built (`DetEntropy::state()` ‖ `DetDevice::snapshot`), and what
it must NEVER do (frame the bare device blob ⇒ `BadLength{16}`). The `tag_for_device_id`
inline comment is updated in lockstep (`0x0004 ... sec_version 2, see RESOLVED above`).
Good doc hygiene — the code tells the next reader exactly why this shape exists.

### P5 — Dependency added as dev-dep with a justifying comment

`dh-devices.workspace = true` lands under `[dev-dependencies]` with a one-line rationale
pointing at the test and bead (`Cargo.toml:21-22`). The integration test correctly lives
in `tests/` (not `src/`), so the engine crate does not take a runtime dependency on the
device crate just to express the seam. Right layering instinct.
