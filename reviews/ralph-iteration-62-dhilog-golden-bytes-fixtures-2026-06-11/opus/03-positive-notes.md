# Positive Notes

## The hash pin is the right freeze primitive, and the failure messages are excellent

The two `*_BLAKE3` constants are hardcoded and compared against the bytes on disk
(`golden.rs:107-111, 130-134`). This is what makes the freeze survive the regen
footgun: I confirmed that even when `DHILOG_REGEN_GOLDEN=1` overwrites the
fixtures with drifted writer output, the hash-pin assertion still fails. The two
distinct messages — `"checked-in fixture changed — the v1.0 freeze is violated"`
(hash pin) vs `"writer output drifted from the frozen v1.0 fixture"`
(re-serialization) — tell a future maintainer exactly which invariant broke and
which direction to fix it. The comment on the constants ("If a test fails here,
the WRITER drifted; fix the writer, do not regenerate") is the correct policy
stated at the point of temptation.

## Module doc establishes the freeze contract unambiguously

The `golden.rs` header doc (`THE V1.0 FORMAT FREEZES HERE ... Any layout change
breaks all three and requires a format-version bump plus NEW fixtures — never
edit the checked-in v1.0 files`) plus the regen instruction scoped to "only for a
NEW format version, into new file names" is exactly the discipline a frozen
binary format needs. The `dhilog.rs` module doc was updated in lockstep
(`dhilog.rs:5-11`) to point at the freeze and re-scope NET_TX/EPOCH_HASH to M5.

## Fixture values are deliberately byte-order-sensitive

The chosen inputs would catch endianness/offset drift, not just presence:

- `clock_num=3, clock_den=2` (distinct, non-trivial) — catches a swapped or
  zero-padded clock field. ✓ verified at offset 0x90.
- `end_icount=1000` (`e8 03`), `end_vns=1500` (`dc 05`) — distinct multi-byte LE
  values at adjacent 8-byte slots; a field swap or wrong offset is visible.
- `encoder_fingerprint=0xFEEDFACECAFEBEEF` — a non-palindromic 8-byte value that
  pins LE order at offset 0x240 unambiguously (`ef be fe ca ce fa ed fe`).
- `frame_hint=7` on the second PAD_SET (not `FRAME_HINT_NONE`) — exercises the
  frame-scheduled branch, distinct from the all-ones sentinel on the first.
- PIO_ANSWER `port=0xD370, value=0x12345678` and the RING_PUSH record bytes
  `DE AD BE EF 01 02 03 04` give recognizable, asymmetric byte patterns.

## net_rx lands with the freeze for the right reason

Adding `net_rx` specifically so the kitchen-sink fixture can cover every
writer-emittable canonical kind (NET_RX was in the frozen kind set but had no
emitter) is the correct scoping — you freeze what you can emit, and the method's
doc says exactly that (`dhilog.rs:183-185`). The 2048 cap and `rflags=0` are
spec-faithful, and the bound check mirrors the existing `dev_event` pattern.

## record_count=11 is correct

11 records = 5 canonical (2×PAD_SET + 3×DEV_EVENT) + 1 NET_RX + 3 AUX
(ENTROPY/TIMER_FIRE — note SDK_EVENT and FRAME_MARK too) + END. Counting from the
builder: pad_set, pad_set, dev_event(RING_PUSH), dev_event(CONS_BUMP),
pio_answer(DEV_EVENT), net_rx, entropy, timer_fire, sdk_event, frame_mark, seal
(END) = 11. The header's `record_count=11` and the parse test's
`assert_eq!(h.record_count, 11)` both match the on-disk byte at offset 0x98
(`0b 00 00 00 00 00 00 00`). ✓

## The minimal fixture exercises the degenerate-zero paths

`v1_minimal.dhilog` deliberately uses `entropy_seed=[0;32]` (continue base PRNG),
`encoder_fingerprint=0` (no SDK digests), `end_snapshot_id=[0;32]` (no end
snapshot), and `stop_reason=0` — pinning the "zeros mean X" conventions from
§3.1, and `minimal_fixture_parses` asserts `record_count=1`,
`encoder_fingerprint=0`, `!has_aux()`, `canonical().count()==0`. Good coverage of
the header+END floor case and the HAS_AUX=0 path (END alone does not set
HAS_AUX).
