# Review: ralph/iteration-42-evtc-snapshot (bead e7y) — EVTC save/restore on DetChannelHost

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-42-evtc-snapshot` vs `main`
- **Bead:** determinism-hypervisor-e7y — "DHSNAP EVTC: save/restore detchannel host state incl. non-reconstructible producer seqs"
- **Scope reviewed:** `crates/dh-devices/src/detchannel.rs` (the only changed file, +242 lines: `snapshot()`, `restore()`, `EVTC_LEN`, `EVTC_VERSION`, and `mod evtc_tests`); cross-checked against guest-sdk `detguest-host/src/channel.rs` `ProducerSeqs` contract, the `PvBlk` DHSNAP precedent in `blk.rs`, and ARCHITECTURE.md §8.3 (restore) / §8.4 (fork).

## Verdict

**APPROVE WITH CHANGES.** The serialization is correct, deterministic, fixed-length, and the producer-seq save/restore — the load-bearing non-reconstructible state this bead exists to capture — is right. Tests, clippy, and fmt are all clean. The one substantive issue is a **reused-slot state leak**: `restore()` rewrites every snapshotted field but silently leaves `metrics`, `last_drain_error`, and the entire `responder` (incl. `TableFaultPlan.hits` occurrence counters) at their pre-restore values. For a fork CHILD (constructed fresh via `new()`) this is harmless, but ARCH §8.3 reuses slots in place, and the sibling `PvBlk` device DOES serialize its anomaly counter (`host_io_errors`). The inconsistency must be resolved — either by serializing/resetting these fields or by an explicit doc contract that the caller calls `new()` per restore.

## Stats

| Metric | Value |
|---|---|
| Files changed | 1 (`crates/dh-devices/src/detchannel.rs`) |
| Lines added | +242 |
| New tests | 2 (`evtc_roundtrips_attached_state_and_seqs`, `evtc_roundtrips_detached_state_and_refuses_bad_input`) |
| `cargo test -p dh-devices` | 61 + 10 + 0 pass, **run 2x, both green** |
| `cargo clippy -p dh-devices --all-targets` | clean (no warnings/errors) |
| `cargo fmt -p dh-devices -- --check` | clean |
| Critical findings | 0 |
| Important findings | 2 |
| Suggestions | 4 |
| Positive notes | 6 |

## Finding counts

- Critical: 0
- Important: 2 (reused-slot metrics/responder leak; missing fork doc breadcrumb)
- Suggestions: 4
