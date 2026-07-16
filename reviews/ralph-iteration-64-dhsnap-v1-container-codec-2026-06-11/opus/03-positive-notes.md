# Positive Notes

### Byte-exact spec fidelity to API.md §4

Every field matches the §4 table with no drift:
- Header: `magic[6]` + `version 0x0100` + `section_count u32` + `_pad u32` = 16
  (`HEADER_LEN = 16`, `dhsnap.rs:33`, `:188-201`).
- Section: `tag[4]` + `sec_version u16` + `_pad u16` + `len u32` (=12) + contents
  + zero-pad to 8 (`SECTION_HEADER_LEN = 12`, `:35`; pad math `(8 - len%8)%8`
  appears identically on the write side `:132` and read side `:224`).
- All 11 tags present in §4 table order (`KNOWN_TAGS`, `:55-67`).
- Unknown-tag rejection on read (`:229-231`) AND write (`:117-119`) — the spec
  only mandates the reader, but rejecting on write too prevents bad blobs at the
  source. Good defensive symmetry.

### `Container::parse` is genuinely total — every index dominated

I traced each slicing operation to a preceding check; there is no reachable
panic:
- `bytes[0..6]`, `[6..8]`, `[8..12]`, `[12..16]` are all guarded by the
  `bytes.len() < HEADER_LEN` early return (`:188`).
- The loop guard `offset < bytes.len()` plus the
  `bytes.len() - offset < SECTION_HEADER_LEN` check (`:206-209`) means
  `bytes.len() - offset` never underflows and `b[0..12]` is always in range.
- `contents_end`/`padded_end` use `checked_add` (`:220-225`), so even a forged
  `len = 0xFFFF_FFFF` on a 32-bit host yields `Truncated`, not an overflow
  panic. The comment correctly notes the sum can't overflow a 64-bit usize but
  keeps `checked_add` for the 32-bit host case — exactly the right reasoning.
- The `bytes.len() - offset < padded_end` check (`:226`) dominates both
  `b[contents_end..padded_end]` (`:235`) and `b[SECTION_HEADER_LEN..contents_end]`
  (`:241`): `padded_end <= bytes.len() - offset = b.len()`.
- `offset += padded_end` (`:243`) cannot overflow because `padded_end <= b.len()`
  ⇒ `offset + padded_end <= bytes.len()`.

The `arbitrary_truncations_never_panic` and `single_byte_corruptions_never_panic`
smokes (`tests:313-329`) sweep every truncation length and every single-byte
flip — the empirical complement to the static argument. This matches the DHILOG
reader-battery rigor the prompt called for.

### Device-id↔tag map cross-checks cleanly

All six live ids match their authoritative constants exactly:

| Map (`dhsnap.rs:73-78`) | Source of truth |
|---|---|
| `0x0001 → EVTC` | `dh-inputlog::dhilog::DEVICE_ID_DETCHANNEL = 0x0001` |
| `0x0002 → CLKD` | `dh-devices::clock::DEVICE_ID_PV_CLOCK = 0x0002` |
| `0x0003 → PADD` | `dh-devices::pad::DEVICE_ID_PV_PAD = 0x0003` |
| `0x0004 → ENTR` | `dh-devices::entropy::DEVICE_ID_PV_ENTROPY = 0x0004` |
| `0x0005 → BLKO` | `dh-devices::blk::DEVICE_ID_PV_BLK = 0x0005` |
| `0x0006 → SERL` | `dh-devices::serial::DEVICE_ID_DEBUG_SERIAL = 0x0006` |

The map being a single `match` in one place honors the `DetDevice` doc's
"one place" rule.

### Typed section layouts mirror their owners exactly

- `EntrSection { seed[32], stream u64, word_pos u128 }` (`:316-320`) is a
  field-for-field mirror of `dh-devices::entropy::EntropyState` (`entropy.rs:58-62`),
  whose fields come straight from `rand_chacha`'s
  `get_seed`/`get_stream`/`get_word_pos`. LEN = 32+8+16 = 56, matching the spec's
  explicit "56 bytes."
- `TimeSection` LEN = 8+8+8+32 = 56, matching §4.
- Both `decode`s validate `sec_version` AND `len` before any field read, and the
  `typed_sections_roundtrip_and_pin_their_layout` test pins each field's exact
  LE byte offset (`tests:82-91`) — a swapped field fails the golden, not just the
  round-trip. This is the right way to freeze a wire layout.

### Arch-neutrality preserved (aarch64-safe)

No KVM types, no `target_arch` gates, no x86-only deps in the crate — the only
mention of KVM is a doc-comment explaining why VCPU contents are *not*
re-serialized here (`:16`). The crate's sole dep is the `snapstore-client`
dev-dependency. The ownership split (contents stay with their owners) is what
keeps KVM types out, exactly as the bead-v5w discipline intends.

### Error model is precise and machine-actionable

`ReadError`/`WriteError`/`SectionError` are `Copy` enums carrying the offending
`tag`/`index`/`found` so callers get actionable diagnostics, and each variant's
doc-comment cites the §4 rule it enforces. `SectionCountMismatch` catches the
case where the framing is valid but `section_count` lies — a subtle integrity
check that a lesser codec would skip.

### Clean tooling

`cargo test -p dh-snapshot` → 17/17 codec tests pass (+1 readiness, 0 doctests).
`cargo clippy -p dh-snapshot --all-targets` → zero warnings. `#![forbid(unsafe_code)]`
is in force at the crate root (`lib.rs:1`).
