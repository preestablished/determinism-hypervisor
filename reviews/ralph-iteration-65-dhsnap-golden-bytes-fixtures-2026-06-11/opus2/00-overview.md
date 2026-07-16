# DHSNAP Golden Fixtures — 2nd-Reviewer Overview

- **Branch:** `ralph/iteration-65-dhsnap-golden-bytes-fixtures` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** 9tl — DHSNAP v1.0 golden-bytes freeze (third instance of the triple-freeze pattern: DHILOG bp9 / iter 62, DHSNAP codec / iter 64)

## Summary

This branch lands the v1.0 byte-freeze for the `DHSNAP` device-blob container:
a kitchen-sink fixture (`v1_kitchen_sink.dhsnap`, 684 B, one section per §4 tag)
and an empty-container fixture (`v1_minimal.dhsnap`, 16 B, header-only), both
BLAKE3-pinned in `tests/golden.rs`, plus a `blake3` dev-dependency and a
`*.dhsnap binary` gitattributes line. The triple-freeze discipline matches bp9:
(1) checked-in bytes hash-pinned, (2) writer re-serializes identical bytes,
(3) reader decodes every section to pinned values.

I verified the fixtures **by running, not reading**:

- Independently recomputed BLAKE3 over both fixture files via a throwaway test
  using the crate's own `blake3` dep. Both pinned constants match exactly
  (`9014b0…3a91` kitchen-sink, `2e9df5…84aa` minimal).
- Hexdumped and parsed both fixtures in Python against the API.md §4 layout:
  16-byte header (magic `DHSNAP`, version `0x0100` LE, count=11, header `_pad`=0),
  12-byte section headers, all 11 tags **in spec table order**, every section
  `_pad` and every alignment-pad byte **zeroed**.
- Confirmed the VCPU kitchen-sink payload is byte-identical to
  `(0..200).map(|i| i ^ 0xA5)`, MCFG = `1..=64`, TIME/ENTR at their 56-byte typed
  layouts, and CLKD(12→16)/PADD(21→24)/EVTC(39→40)/BLKO(36→40) padded correctly.
- Confirmed `v1_minimal.dhsnap` is byte-identical to
  `b"DHSNAP" + pack("<HII", 0x0100, 0, 0)` — i.e. `ContainerWriter::new().finish()`
  exactly, parsing to zero sections.
- Confirmed all section lengths agree with the sibling `dhsnap_codec.rs` test, and
  that EVTC's 39-byte length is the *real* owner length
  (`dh-devices::detchannel::EVTC_LEN = 4+4+4+5+5+1+16 = 39`), not an arbitrary value.

## Verdict

**APPROVE.** The freeze is correct, the bytes match the spec, the hashes are
independently reproduced, the anti-laundering note is present and on par with bp9,
and the diff is tightly scoped to the bead with zero drift. No blocking issues.
Two minor (non-blocking) notes on the in-file expression duplication and a stale
spec-table entry that this bead surfaces but does not own.

## Stats

| Metric | Value |
|---|---|
| Files changed | 6 (+197 lines, 2 binary fixtures) |
| Tests run | `cargo test -p dh-snapshot` — 18 + 4 + 1 pass, 0 fail |
| Independent hash legs verified | 2/2 match |
| Critical | 0 |
| Important | 0 |
| Suggestions | 3 |
