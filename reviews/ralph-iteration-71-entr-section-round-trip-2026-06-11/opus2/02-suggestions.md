# Suggestions (non-blocking)

### S1 — Cover `EntrSectionV2::decode`'s `BadVersion` branch directly

- **File:** `crates/dh-snapshot/src/dhsnap.rs:425-427`; test in
  `crates/dh-snapshot/tests/entr_roundtrip.rs`

The v2 `decode` has a `BadVersion { found }` early return (line 427), but no test exercises
it with the *right length and the wrong version*. The committed misuse test
(`v1_and_v2_sections_coexist_and_misuse_is_loud`) only feeds a 16-byte blob at version 2
(length AND version both wrong, so `BadVersion` fires first and `BadLength` is never seen)
and a v1 section (version 1) into the v2 decoder. A correctly-sized 72-byte buffer at, say,
version 3 would confirm `BadVersion` precedes the length check. Cheap one-liner:

```rust
assert_eq!(
    EntrSectionV2::decode(&[0u8; EntrSectionV2::LEN], 3),
    Err(SectionError::BadVersion { found: 3 })
);
```

The sibling `EntrSection`/v1 and `TimeSection` already have analogous `BadVersion` coverage
in `dhsnap_codec.rs:318,326`, so this just brings v2 to parity.

### S2 — Pin a known-answer PRNG vector somewhere (see I2)

Restated as a suggestion in case I2 is judged out-of-scope for a bead: even a single
`#[test]` asserting `seed=[0x42;32] → a4ddf31f…` (the exact bytes are in 01/I2) closes the
one gap the golden fixtures leave — they pin DHILOG/DHSNAP framing and digests but never
the raw ChaCha20 output. This is the cheapest possible insurance against a silent
`rand_chacha` stream change breaking fork-and-replay determinism.

### S3 — `from_parts`/`decode` use `.expect("8")` where the slice length is already guaranteed

- **File:** `crates/dh-snapshot/src/dhsnap.rs:401-403, 432-440`

Minor style: after the `device_regs.len() != 16` / `bytes.len() != Self::LEN` guards, the
`try_into().expect("N")` calls are infallible. This matches the existing house style in the
v1 `EntrSection::decode` (which uses `.unwrap()` at line 356-358), so it's consistent — but
the mixed `.expect("8")` (new code) vs `.unwrap()` (v1, same file) is a small inconsistency.
Pick one spelling for the file. Not worth a round-trip on its own; fold into any future
touch.
