# Action Items

## Critical

_None._ The framing codec is total and correct over untrusted bytes; all
adversarial probes pass and `cargo test -p dh-snapshot` is green.

## Important

- [ ] **ENTR tag collision — flag concretely for bead 6yl (not a 68l code fix).**
  `tag_for_device_id(0x0004) -> ENTR` (`dhsnap.rs:76`) routes the pv-entropy
  device's 16-byte register blob (`buf_gpa u64 || len u32 || status u32`,
  `crates/dh-devices/src/entropy.rs:168-172`) to the `ENTR` tag, whose §4 / 
  `EntrSection` contents are the **56-byte** ChaCha20 PRNG state
  (`dhsnap.rs:310`, API.md:618). Two incompatible payloads, one tag; a generic
  engine framing device 0x0004 via this map emits a spec-violating 16-byte ENTR
  that `EntrSection::decode` rejects (`BadLength{found:16}`). There is **no §4
  tag** anywhere for the device's `{buf_gpa,len,status}` regs. **Decision for
  6yl:** the only fork-critical entropy state is the 56-byte PRNG tuple (VMM-owned,
  `DetEntropy::state()`); the device regs are guest-reconstructible and likely
  need not survive restore. So the engine must special-case 0x0004 — take ENTR
  contents from the VMM PRNG state, NOT from `DetDevice::snapshot(0x0004)` — or, if
  the regs must persist, merge them into a `sec_version`-bumped ENTR layout
  (bump `EntrSection::VERSION` + the §4 table together). File/annotate on bead 6yl.

- [ ] **Add a one-line carve-out doc note at `dhsnap.rs:76`** so the next reader
  of the device-id map isn't misled: `ENTR` contents are VMM-owned PRNG state, not
  this device's `snapshot()` regs (see bead 6yl).

## Suggestions

- [ ] **Settle golden-bytes labor with bead 9tl (P0, now unblocked).** This bead's
  `golden_bytes_minimal_container` (`tests/dhsnap_codec.rs:121`) hand-assembles
  `expect` in-test, so writer + builder can drift together undetected — it is a
  layout-offset pin, not a freeze. 9tl should own checked-in `.dhsnap` fixtures
  with BLAKE3 pins and the DHILOG anti-laundering rule (`crates/dh-inputlog/tests/
  golden.rs`: never regenerate + re-pin in one PR). The `full_container()` helper
  here is a ready-made kitchen-sink generator — note that in 9tl's description.

- [ ] **Add a completeness test** asserting `KNOWN_TAGS.len() == 11` and that every
  `tag_for_device_id` Some-value is in `KNOWN_TAGS` — closes the gap where a 12th
  tag added to the `tag` module / map / array but not all three goes uncaught.

- [ ] **Decide canonical write order enforcement.** Engine-fixed order + doc is
  acceptable for v1 (one engine), but a silently-diverging snapshot ref is the
  exact failure the ref is sensitive to. Consider a `finish()` assertion that
  `self.seen` is a subsequence of `KNOWN_TAGS`, or make "the engine fixes order" a
  testable invariant in the engine bead. Document the load-bearing assumption at
  the `push_section` docstring, not only the module header.

- [ ] **(Optional) `TrailingBytes { offset }` variant.** Trailing slop shorter than
  a section header reports `Truncated { index: <next ordinal> }`, which names a
  section that doesn't exist (matches DHILOG `seq_for_err`; harmless). Only worth
  it if divergence/forensic tooling reads these variants.

- [ ] **(Optional) Note the allocation trade.** `Container::parse` builds an owned
  `Vec<Section>`; DHILOG's reader is allocation-free over borrowed payloads. Fine
  for ≤ 11 sections — only revisit if `no_std`/alloc-free parity becomes a goal.
