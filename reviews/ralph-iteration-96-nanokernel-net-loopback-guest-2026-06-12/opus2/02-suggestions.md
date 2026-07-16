# Suggestions (non-blocking)

## 1. The drift test's `frame[63]` assertion silently assumes `FRAME_LEN >= 64`
`tests/nanokernel/tests/elf_shape.rs:514` indexes `frame[63]` directly. It is
correct today (`FRAME_LEN == 64`), but if a future bead *shrinks* `FRAME_LEN`
below 64 the test panics with an out-of-bounds index rather than a clear drift
message. Cheap hardening — index the last element instead so the assertion
tracks `FRAME_LEN` automatically:

```rust
let last = frame.len() - 1;
assert_eq!(frame[last], NET_LOOPBACK_FRAME_BYTE_BASE.wrapping_add(last as u8));
```

This also keeps the `wrapping_add` honest if someone later bumps `FRAME_LEN`
past 196 (where `0x5A + 196` wraps) — the current `wrapping_add(63)` happens not
to exercise the wrap, so the test wouldn't catch a helper that used a
non-wrapping cast. Indexing the last byte at a wrapping length would.

## 2. `loop` caps the spin at 65536 iterations — fine, but make the budget's units explicit
`SPIN_BUDGET = 65536` is a poll *count*, and each poll is one MMIO-read VM exit
serviced by the run loop. The header explains the intent well, but a one-word
addition to the `%define` comment ("65536 poll exits") would save the next
reader from wondering whether it is cycles, instructions, or exits. Trivial doc
nicety.

## 3. `repe cmpsb` mismatch path doesn't distinguish "wrong byte" from "short read"
`net_loopback.asm:114-118`: a mismatch jumps to `.fail_x` ('x'). Since `RX_LEN`
was already verified `== FRAME_LEN`, a content mismatch is the only reachable
cause, so a single 'x' is adequate. No change needed — noting only that if RX
ever gained partial-delivery semantics, 'x' would conflate two failures. Given
`net.rs` copies-or-errors (no partials), this stays correct.

## 4. Consider asserting `FRAME_BYTE_BASE` non-zero-ness isn't relied on
Minor: the comment "RX_LEN starts 0 (zeroed RAM-like reset)" assumes guest RAM
is zeroed at entry. crt0 guarantees a zeroed `.bss`, but `TX_GPA`/`RX_GPA` are
raw GPAs outside the program image, not `.bss`. The guest never *reads*
uninitialized RX bytes before delivery overwrites them (it gates on `RX_LEN`
first), so there is no real dependency — but the parenthetical could mislead a
future reader into assuming arbitrary-GPA zeroing is guaranteed. A word like
"(RX_LEN reg starts 0 per PvNet::new)" would be more precise than "RAM-like".
