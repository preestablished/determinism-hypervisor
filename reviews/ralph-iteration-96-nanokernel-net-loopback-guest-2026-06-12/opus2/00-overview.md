# net_loopback nanokernel guest — second-reviewer overview

- **Branch:** `ralph/iteration-96-nanokernel-net-loopback-guest` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 4 files, +237 / -0, 1 commit (`86fb745`)

## What the change does

Adds the M5 `net_loopback` nanokernel guest (bead `fbr`) that exercises the
pv-net loopback end to end:

1. Validates BootInfo and that guest RAM covers both buffers
   (`mem_size >= RX_GPA + RX_CAP_BYTES`).
2. Fills a known frame at `TX_GPA` (byte `i = (0x5A + i) & 0xFF`).
3. **Publishes the RX buffer before ringing TX** (so a delivery can never
   race publication).
4. TXes the frame, checks `TX_STATUS == OK`, emits `'T'`.
5. Bounded-spins polling `RX_LEN` (budget 65536), emits `'R'` when it equals
   `FRAME_LEN`.
6. `repe cmpsb`-verifies RX bytes against TX bytes, clears `RX_LEN`, emits `'X'`.

Failures emit a lowercase letter (`t`/`r`/`x`) and park. Full success serial
is `"TRX"`.

Supporting changes: `build.rs` registers the program; `lib.rs` adds the
`net_loopback_elf()` accessor, the `NET_LOOPBACK_*` GPA/frame constants, the
`NET_LOOPBACK_OK_SEQUENCE`, and a `net_loopback_frame()` helper; `elf_shape.rs`
adds the static-exec shape check plus a drift pin (`%define`s vs device-side
`dh_devices::net::*` and the lib.rs constants, plus compile-time fit/disjoint
asserts).

## Verdict

**APPROVE.** No Critical or Important issues. I independently exercised every
implicit assumption the prompt flagged — the spin/`loop` register usage, the
RX_LEN stickiness vs the "harness lands NET_RX between polls" race, the
`mem_size` ordering vs the frame fill, the `inc al` / wrapping-`as u8`
equivalence, the RX_VECTOR-stays-zero assumption, the `STATUS_OK`
value-vs-offset pin, and the single-shot record/replay adequacy — and all hold.
The implementation is correct, the drift pin is genuinely load-bearing, and the
asm faithfully matches the device contract in `crates/dh-devices/src/net.rs`.
A few minor robustness/maintainability suggestions only (see `02-suggestions.md`).
