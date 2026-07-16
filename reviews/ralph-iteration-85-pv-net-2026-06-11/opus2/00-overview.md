# pv-net Loopback Device — Review Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-85-pv-net`
- **Bead:** determinism-hypervisor-mmv (pv-net loopback, ARCH §6.7, MMIO 0xD000_5000)
- **Scope:** 4 files, +449 lines

## Summary

This iteration lands the **pv-net loopback device** (`crates/dh-devices/src/net.rs`,
422 lines) plus the supporting log plumbing. The design is clean and the
determinism story is sound:

- **TX is an OUTPUT.** The doorbell reads `tx_len` bytes from `tx_buf_gpa`,
  computes `digest8` (first 8 bytes of BLAKE3, LE — identical to ENTROPY /
  SDK_EVENT), emits the AUX `NET_TX` record (`len` + `digest8`), and buffers
  **no frame**. Replay re-executes the doorbell deterministically and the
  verifier compares digests. The TX path is replay-pure: its only state is
  `tx_status`, which is a snapshotted register.
- **RX is an INPUT.** It happens *only* via `apply_net_rx`, the run-control
  entry point (mirroring `PvPad::apply_pad_set`), driven by canonical
  `NET_RX` log records. The frame is copied into the guest-published RX buffer,
  `rx_len` is set, and the edge vector (if enabled) is returned for injection.
- **NETL section is 36 bytes of pure registers** — no queue, so the §4
  "pending-RX-must-be-empty-at-snapshot" rule holds by construction. A nonzero
  `rx_len` at snapshot is fine: it is a delivered-length register, not pending
  state.
- **DEVICE_ID_PV_NET = 0x0007** matches `dh-snapshot`'s `device_id → tag::NETL`
  map exactly (`crates/dh-snapshot/src/dhsnap.rs:93`).

**Writer/reader byte agreement verified**: `LogWriter::net_tx` writes `len`@0..4
and `digest8`@8..16 (bytes 4..8 are zero pad); `reader.rs:192` decodes
`len: u32at(0)`, `digest8: u64at(8)`. Byte-for-byte symmetric, and the length
validator (`reader.rs:537`) requires exactly 16 bytes. ✓

**Build/test state**: `cargo test -p dh-devices --lib` → **75 passed, 0 failed**,
including the 7 new pv-net tests and the `deny_list_gate` (which scans
`src/*.rs` via `read_dir`, so `net.rs` is auto-covered — no host ambient
authority). `cargo clippy -p dh-devices --lib` is clean.

## Verdict

**Approve with follow-ups.** The device itself is correct, deterministic, and
well-tested. There are **no Critical correctness bugs**. The one finding that
genuinely matters is an **Important seam gap for the dependent bead y78**: the
module doc promises subscribers will "re-read the frame from guest RAM through
the still-live TX regs," but `PvNet` exposes **no public accessor** for
`tx_buf_gpa` / `tx_len` (contrast `PvPad::frame_counter()`). y78 cannot
implement the loopback without either an MMIO-read dance against a synthesized
`DevCtx` or a new `pub fn tx_regs()`. Surfacing this now avoids a churn cycle
when y78 lands.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 2     |
| Suggestions| 4     |
| Positive notes | 6 |

| Metric | Value |
|--------|-------|
| Files changed | 4 |
| Lines added | +449 |
| New unit tests | 7 |
| Crate tests passing | 75 / 75 |
| Clippy | clean |
