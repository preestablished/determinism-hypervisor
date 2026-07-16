# Positive Notes

## Triple-freeze faithfully replicates the established pattern

`golden.rs` implements all three legs cleanly:
1. **Byte pin** — `KITCHEN_SINK_BLAKE3` / `MINIMAL_BLAKE3` literal constants
   (`golden.rs:38-41`).
2. **Writer pin** — `build_kitchen_sink() == fixture` and `build_minimal() ==
   fixture` (`golden.rs:111-117`, `:127-133`).
3. **Reader pin** — `kitchen_sink_fixture_parses_to_pinned_sections` decodes every
   one of the 11 sections and `minimal_fixture_parses_empty` asserts zero sections +
   `len == HEADER_LEN` (`golden.rs:135-192`).

The structure is a clean copy of the bp9 DHILOG discipline, including identical
anti-laundering wording — good cross-instance consistency.

## The hash pin is a genuinely independent leg

I reproduced both BLAKE3 digests out-of-band (a throwaway test using the crate's own
`blake3` dep) and they matched the pinned constants exactly. The fixtures are not
self-certifying: a writer regression that re-serializes different bytes fails the
hash assert immediately. This is the property the whole pattern hinges on, and it
holds.

## Byte layout is exactly to spec

Independent Python parse confirmed, against API.md §4:
- Header: `DHSNAP` / `0x0100` LE / count=11 / `_pad`=0.
- All 11 tags in **spec table order** (MCFG, VCPU, LAPC, TIME, ENTR, CLKD, PADD,
  EVTC, BLKO, NETL, SERL).
- Every section `_pad`=0 and every 8-byte alignment pad byte zeroed — the
  reserved-means-zero rule the reader enforces.
- TIME/ENTR carry their real typed encodings (deliberately byte-order-sensitive
  little-endian values like `0x0102_0304_0506_0708`), so the freeze actually pins
  endianness, not just length.

## Section sizes are owner-accurate and internally consistent

Every kitchen-sink section length matches the sibling `dhsnap_codec.rs` test
(LAPC 40, CLKD 12, PADD 21, EVTC 39, BLKO 36, MCFG 64, VCPU 200, TIME/ENTR 56). The
EVTC 39-byte length is the *real* `dh-devices::detchannel::EVTC_LEN`, and the empty
NETL/SERL sections honor their "must be empty" rules — so the framing fixture mirrors
the production owner shapes rather than using round numbers.

## Minimal fixture is the canonical empty container

`v1_minimal.dhsnap` is byte-identical to `ContainerWriter::new().finish()` (16 B,
count 0) and parses to zero sections — a tight, unambiguous header-only anchor.

## Clean, well-scoped diff and correct binary handling

The diff is exactly the bead: gitattributes line, `blake3` dev-dep (workspace-pinned,
matching the sibling `dh-inputlog`/`dh-vmm` usage), two fixtures, one test file. No
drift. `*.dhsnap binary` lands correctly (`git check-attr` + `git ls-files --eol`
both confirm binary treatment), and fixtures are committed non-executable (`100644`).
The module doc is unusually thorough about *why* the freeze exists and how to (not)
regenerate it.
