# Review: pv-net loopback device (iteration 85)

- **Branch:** `ralph/iteration-85-pv-net`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** `determinism-hypervisor-mmv` — pv-net loopback (ARCH §6.7, window `0xD000_5000`)

## Scope

4 files, +449 lines, 1 commit:

1. `crates/dh-devices/src/net.rs` (new, 422 lines) — `PvNet` `DetDevice`: one-deep pv-blk-style
   TX registers (`TX_BUF_GPA`/`TX_LEN`/`TX_DOORBELL`/`TX_STATUS`) + RX registers
   (`RX_BUF_GPA`/`RX_CAP`/`RX_LEN`/`RX_VECTOR`), the `doorbell()` that logs AUX `NET_TX`
   (len + digest8) and buffers no frame, and `apply_net_rx()` (PAD_SET-style run-control entry).
   `DEVICE_ID_PV_NET = 0x0007`, 36-byte NETL section, 7 unit tests.
2. `crates/dh-inputlog/src/dhilog.rs` — `LogWriter::net_tx()` (AUX `KIND_NET_TX` 0x44, 16-byte payload).
3. `crates/dh-devices/src/ctx.rs` — `DevCtx::log_net_tx()`.
4. `crates/dh-devices/src/lib.rs` — `pub mod net;`.

## Summary

This is a clean, well-reasoned, pattern-faithful device. It mirrors `PvBlk` (one-deep
doorbell registers, synchronous completion via `STATUS`) for TX and `PvPad::apply_pad_set`
(identical `Result<Option<u8>, _>` vector signature, identical u8 masking on the vector
register) for RX. The central design idea — buffering **no** frame so that the NETL
section is registers-only and the API.md §4 "pending-RX state must be empty at snapshot"
rule holds *by construction* — is sound and elegant: there is literally no queue that can
be non-empty, so the invariant cannot be violated. The AUX `NET_TX` payload shape
(`len u32 ‖ _pad u32 ‖ digest8 u64`, 16 bytes) matches exactly what the reader pins
(`reader.rs:537` requires `payload.len() == 16`; `RecordBody::NetTx { len: u32at(0),
digest8: u64at(8) }` at `reader.rs:192`) and what API.md §3.3 row `0x44` documents.
`DEVICE_ID_PV_NET = 0x0007` matches `dhsnap.rs:93` `tag_for_device_id(0x0007) => NETL`.
The deny-list source-grep gate (`lib.rs` `no_host_ambient_authority`) scans every `.rs`
in `src/`, so `net.rs` is covered automatically — and it passes (verified: 75 tests green,
including all 7 net tests and the gate).

Build and tests verified green locally (`cargo test -p dh-devices --lib`: 75 passed).

The findings below are not blockers for what this bead set out to do. The two worth
acting on are documentation-shaped: (a) a §4 NETL row divergence to record in the `veu`
ledger, and (b) making the per-exit-drain contract explicit in the module doc to close
the double-doorbell determinism reasoning. There is also a genuine (if currently
latent) log/device asymmetry around zero-length `NET_RX`.

## Verdict

**APPROVE** — with two recommended documentation follow-ups (one `veu` ledger entry,
one module-doc sentence) and one logged issue (zero-length NET_RX asymmetry) that can be
tracked as a bead rather than fixed in this device.

## Stats

| Metric | Value |
| --- | --- |
| Files changed | 4 |
| Lines added | +449 |
| New device tests | 7 (all passing) |
| Crate test total | 75 passing, 0 failing |
| Critical findings | 0 |
| Important findings | 2 |
| Suggestions | 4 |
