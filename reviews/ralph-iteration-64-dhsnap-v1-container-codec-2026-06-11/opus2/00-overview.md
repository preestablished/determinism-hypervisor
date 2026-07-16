# DHSNAP v1 Container Codec — Review (2nd reviewer)

- **Branch:** `ralph/iteration-64-dhsnap-v1-container-codec` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** 68l — DHSNAP v1 container codec (`crates/dh-snapshot`)
- **Spec:** API.md §4 (`.agents/docs/determinism-hypervisor/API.md`)
- **Diff:** `/tmp/ralph64-diff.txt` (684 insertions; 3 files)

## Summary

This bead adds the DHSNAP v1 container codec: a `ContainerWriter` (append-only,
header back-filled on `finish`), a `Container::parse` total decoder over
untrusted bytes, the §4 tag table, the device-id↔tag map (single source of
truth), and typed `TimeSection` / `EntrSection` structs. The framing layer is
genuinely good: the decoder is total, the overflow reasoning is correct, every
§4 validation rule has a byte-surgery negative test, and the codec matches the
DHILOG reader (iteration 61) conventions closely (same error-enum shape,
`TooShort`/`BadMagic`/`UnsupportedVersion`/`Truncated`/`NonzeroPadding`/
`*CountMismatch` naming, reserved-means-zero rejection, totality smokes).

I ran the documented adversarial probes as scratch tests (not committed):

| Probe | Result | Verdict |
|---|---|---|
| 21-byte contents, 3 pad bytes cut off at EOF | `Truncated{index:0}` | correct |
| forged `len = 0xFFFF_FFFF` (4 GiB−1) | `Truncated{index:0}`, no panic | correct |
| `len = 5`, pad omitted at EOF | `Truncated{index:0}` | correct |
| empty container, `count = 0` | `Ok` (0 sections) | correct |
| `count = 5`, no sections | `SectionCountMismatch{5,0}` | correct |
| `count = 0xFFFF_FFFF`, 1 real section | `SectionCountMismatch`, no alloc DoS | correct |
| two empty sections (zero-len) | advances by 12B each, no infinite loop | correct |
| exact-fit single section | clean loop exit, no underflow | correct |
| version `1.255` / `0.0` | accepted / `UnsupportedVersion` | correct |
| trailing 3 garbage bytes (< section header) | `Truncated{index:1}` | correct (index off-by-one, cosmetic) |

`cargo test -p dh-snapshot`: **17 passed, 0 failed** (plus the readiness test).

The one finding that matters is **not** a codec bug — it is a spec/integration
landmine the codec sits on top of: the pv-entropy device's `snapshot()` emits a
16-byte register blob, and `tag_for_device_id(0x0004)` maps that device to
`ENTR`, whose §4 contents are the 56-byte PRNG state. Two different payloads,
one tag. This codec is correct in isolation but cannot be wired to the entropy
device as-is. It belongs to bead 6yl and is documented below as Important.

## Verdict

**APPROVE.** The framing codec is correct, total, and well-tested; no Critical
or blocking issues in the diff under review. One Important item is a forward
hazard for the integration bead (6yl), not a defect in 68l's code — the codec
deliberately scopes itself to framing + typed sections and flags the boundary.
The golden-bytes division of labor with bead 9tl should be settled (see 02).

## Stats

- Critical: 0
- Important: 1 (ENTR tag collision — for bead 6yl, not a 68l code defect)
- Suggestions: 5
- Files reviewed: `src/dhsnap.rs` (353), `tests/dhsnap_codec.rs` (329), `src/lib.rs` (+2)
- Adversarial scratch probes run: 14 (all pass), in `/tmp/dhsnap_probe` (not committed)
