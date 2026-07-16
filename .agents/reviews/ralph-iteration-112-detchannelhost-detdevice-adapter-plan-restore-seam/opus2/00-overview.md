# Review Overview

- Branch: `ralph/iteration-112-detchannelhost-detdevice-adapter-plan-restore-seam`
- Date: 2026-06-15
- Reviewer: Local Subagent (2nd reviewer)
- Overall verdict: REQUEST_CHANGES

This branch adds a `DetChannelDevice` `DetDevice` adapter around `DetChannelHost`, exposes the detchannel device id for DHSNAP `EVTC`, and adds restore-engine coverage showing an EVTC section can be snapshotted and restored after RAM is materialized. The broad direction is sound: the adapter keeps the live PIO ABI separate from the MMIO bus while letting the existing DHSNAP device loop carry detchannel host-only state.

The main issue is restore strictness. EVTC is now consumed by the generic snapshot restore path, but `DetChannelHost::restore` accepts non-canonical flag bytes and inconsistent internal state. A corrupted or future-incompatible EVTC section can silently restore as a different channel state instead of returning `RestoreError`, which cuts against the restore engine's "shape strictness" contract.

## Stats

- Files changed: 7
- Lines added/removed: +336/-10
- Commits: 1
- Commit history: `96db9c6 ralph: iteration 112 checkpoint - detchannel device adapter`

## Review Context

- Reviewed the committed branch diff `main...HEAD`.
- Read the full changed files:
  - `Cargo.lock`
  - `crates/dh-devices/src/detchannel.rs`
  - `crates/dh-devices/src/lib.rs`
  - `crates/dh-worker/Cargo.toml`
  - `crates/dh-worker/src/restore_engine.rs`
  - `crates/dh-worker/tests/common/mod.rs`
  - `crates/dh-worker/tests/restore_engine.rs`
- Checked related context in `crates/dh-worker/src/snapshot_engine.rs`, `crates/dh-worker/src/fork_engine.rs`, `crates/dh-snapshot/src/dhsnap.rs`, `crates/dh-devices/src/bus.rs`, `crates/dh-vmm/src/recording.rs`, and the guest-sdk `detguest-host` channel/inject code.
- Ran:
  - `cargo test -p dh-devices detchannel_device -- --nocapture`
  - `cargo test -p dh-worker --test restore_engine restore_device_loop_reattaches_detchannel_evtc_after_ram_load -- --nocapture`
  - `cargo test -p dh-devices`
  - `cargo test -p dh-worker --test restore_engine -- --nocapture`
  - `cargo test -p dh-worker --test fork_engine -- --nocapture`
