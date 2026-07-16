# Review: net_loopback nanokernel guest (iteration 96)

- **Branch:** `ralph/iteration-96-nanokernel-net-loopback-guest` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Stats:** 4 files, +237 / -0, 1 commit (`86fb745`)

## Summary

Adds `net_loopback`, the M5 NET_RX landing guest (bead **fbr**), to the
nanokernel test-guest suite. The guest:

1. Validates `BootInfo` (magic + enough RAM for both buffers).
2. Builds a known 64-byte frame at `0x20_0000` (byte `i = (0x5A + i) & 0xFF`).
3. Publishes an RX buffer at `0x21_0000` (cap 2048) **before** ringing TX —
   so a delivery can never race publication.
4. Rings the pv-net TX doorbell, gates on `TX_STATUS == STATUS_OK`, emits `'T'`.
5. Bounded-spins (65 536 polls) on `REG_RX_LEN`, emits `'R'` once `RX_LEN`
   goes nonzero and equals `FRAME_LEN`.
6. Verifies the delivered payload byte-for-byte with `repe cmpsb` against the
   sent frame, clears `RX_LEN` (consumer's job per the device contract),
   emits `'X'`.
7. Lowercase `t`/`r`/`x` on the corresponding stage failure; the spin is
   bounded so a non-delivering harness gets a loud `'r'`, never a hang.

The change is rounded out by a `build.rs` registration, a `lib.rs` accessor +
constants + frame helper, and an `elf_shape.rs` drift-pin test that compares
every register `%define` against the `dh_devices::net` device truth and the
GPAs/frame params against the `lib.rs` constants, plus const-asserts that the
frame fits both caps and the two buffers are disjoint.

## Assessment of asm correctness

All the load-bearing details check out against `crt0.asm`, `bootinfo.inc`,
the sibling guests, and `crates/dh-devices/src/net.rs`:

- **8-byte vs 4-byte MMIO writes:** GPA registers (`REG_TX_BUF_GPA`,
  `REG_RX_BUF_GPA`) written 8 bytes via `mov [r8+off], rax`; LEN/CAP/DOORBELL
  written 4 bytes via `mov dword`. Matches the device's
  `(REG, len)` match arms exactly (`(REG_TX_BUF_GPA, 8)`, `(REG_TX_LEN, 4)`…).
- **Fill loop:** `mov ecx, 64` zero-extends to `rcx = 64`; `loop` decrements
  `rcx`; `inc al` is byte-width so byte `i = 0x5A + i`, matching
  `net_loopback_frame()`. AL never wraps at len 64; helper's `wrapping_add` is
  consistent (and the drift test exercises `frame[63]` with `wrapping_add(63)`).
- **Spin loop:** `mov rcx, 65536`; on `RX_LEN==0` falls to `loop .spin`; when
  `rcx` hits 0 it falls through to `.fail_r`. Correct bounded poll.
- **repe cmpsb / jne:** `cld` in `crt0` guarantees DF=0; `ecx=64`; on a full
  match `repe` leaves ZF=1 so `jne .fail_x` is not taken. Correct.
- **TX_STATUS gate:** synchronous `STATUS_OK` after the doorbell exit matches
  `PvNet::doorbell`; the guest correctly gates on it before claiming `'T'`.
- **RX delivery semantics:** `apply_net_rx` copies the frame into the published
  buffer and sets `RX_LEN`; the guest polls `RX_LEN`, checks `== FRAME_LEN`,
  verifies the copied bytes, then clears `RX_LEN` (write-any-value ack). All
  consistent with the device.

## Bead fit

- **fbr** ("writes a known frame to TX, spins for RX delivery, verifies
  payload, reports via serial; HOST-RUNNABLE build"): **met.** The guest is a
  standard static x86-64 exec at the load addr (drift test registers it), and
  the TRX/`txr` serial contract is exactly the described behavior.
- **czq** (M5 ACCEPT: NET_RX landing recorded + replayed bit-identically): this
  guest is the *fixture* that blocker depends on. It deterministically TXes one
  frame and consumes exactly one RX delivery with a fixed, drift-pinned byte
  pattern — precisely the stable, recomputable workload a record-then-replay
  bit-identity test needs. It **serves** czq well.

## Verdict

**APPROVE**

No correctness defects found in the asm or the Rust scaffolding. The drift pin
is thorough (device-side register truth + harness-side constants + const
asserts). Remaining notes are non-blocking suggestions and discussion points.
