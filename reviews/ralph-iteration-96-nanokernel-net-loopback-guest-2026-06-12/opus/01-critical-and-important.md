# Critical and Important findings

**None.**

I verified the asm against `crt0.asm` (entry contract, `cld`/DF=0, zeroed
.bss/RAM, HLT park), `bootinfo.inc` (field offsets), the sibling guests
(`device_exercise.asm`, `capture_fixture.asm`, `pad_echo.asm` patterns), and
the device truth in `crates/dh-devices/src/net.rs`. Specifically checked:

- MMIO write widths: 8-byte for `REG_TX_BUF_GPA`/`REG_RX_BUF_GPA`, 4-byte for
  `REG_TX_LEN` / `REG_RX_CAP` / `REG_TX_DOORBELL` / `REG_RX_LEN`. All match the
  device's `(offset, len)` match arms — a mismatched width would fall through
  to the device's `_ => {}` and silently drop the write. No such mismatch.
- `loop` / `rcx` usage in both loops (fill `ecx=64`, spin `rcx=65536`): the
  `mov ecx, imm` zero-extends, `loop` decrements `rcx`, and the spin correctly
  falls through to `.fail_r` when the budget is exhausted.
- `repe cmpsb` + `jne` semantics: DF=0 from `crt0`'s `cld`, ZF=1 on full match,
  so `jne .fail_x` is not taken on success. Correct.
- Fill-loop byte sequence (`0x5A + i`) matches `net_loopback_frame()`, and
  `frame_len 64 <= MAX_FRAME 2048` and `<= RX_CAP 2048`, so `apply_net_rx`
  cannot reject this frame as `FrameTooBig`.
- RX buffer published (nonzero GPA, nonzero CAP) before TX — `apply_net_rx`
  will not hit `NoRxBuffer`/`FrameTooBig`/`MemFault` for this workload.
- Buffers disjoint: TX `[0x20_0000, 0x20_0040)` vs RX `[0x21_0000, 0x21_0800)`
  — no overlap, and both within the asserted `mem_size` floor.

No blocking issues.
