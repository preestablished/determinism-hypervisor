# Action items

### Critical
_None._

### Important
_None._

### Suggestions (all optional, non-blocking)

- [ ] In `tests/nanokernel/tests/elf_shape.rs` (~line 514), replace the
  hard-coded `frame[63]` assertion with a last-element index
  (`let last = frame.len() - 1; assert_eq!(frame[last],
  NET_LOOPBACK_FRAME_BYTE_BASE.wrapping_add(last as u8));`) so the drift test
  survives a future `FRAME_LEN` change and actually exercises the
  `wrapping_add` if `FRAME_LEN` ever exceeds 196.
- [ ] In `tests/nanokernel/asm/net_loopback.asm`, add "poll exits" to the
  `SPIN_BUDGET` `%define` comment so the budget's units are unambiguous.
- [ ] In `tests/nanokernel/asm/net_loopback.asm:83`, tighten the
  "RX_LEN starts 0 (zeroed RAM-like reset)" comment to reference the register's
  `PvNet::new` reset rather than implying arbitrary-GPA RAM is zeroed
  (the guest does not depend on RAM zeroing — it gates on `RX_LEN` first).

### Verification performed (no action needed — recorded for the next reviewer)

- [x] Spin loop `loop`/`RCX` vs `mov eax,[RX_LEN]`: no clobber, correct.
- [x] "NET_RX lands between polls": impossible to miss — `RX_LEN` is sticky
  (`net.rs:159`, cleared only by the guest at `net.rs:196`).
- [x] `mem_size` check precedes the `TX_GPA` fill; `TX_GPA < RX_GPA` so the
  fill region is covered.
- [x] `inc al` ≡ `(u32 + i) as u8` for all `FRAME_LEN <= 256`.
- [x] `RX_VECTOR` unreferenced by the asm; `rx_vector` starts 0 in
  `PvNet::new`; `apply_net_rx` returns `None`. Polling path consistent.
- [x] `STATUS_OK` pinned as a value via `u32::try_from`, used only in a value
  compare.
- [x] Single-shot guest = exactly one NET_TX + one NET_RX = adequate for
  bit-identical record/replay.
- [x] No partial/oversize RX delivery possible (`net.rs:154` copies-or-errors).
