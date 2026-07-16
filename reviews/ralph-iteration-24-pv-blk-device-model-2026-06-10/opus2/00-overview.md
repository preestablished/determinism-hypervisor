# pv-blk device model — second-reviewer overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-24-pv-blk-device-model` vs `main`
- **Bead:** determinism-hypervisor-sai
- **Scope reviewed:** `crates/dh-devices/src/blk.rs` (new, 624 LOC), `crates/dh-vmm/src/blkfile.rs` (new, 186 LOC), `crates/dh-devices/src/lib.rs` (+1), `crates/dh-vmm/src/lib.rs` (+1). Cross-read: `bus.rs`, `ctx.rs`, `dh-devices/src/lib.rs` deny-list, ARCHITECTURE.md §6.5.

## Summary

This is a clean, well-reasoned CoW block device: a one-deep register set (SECTOR/BUF_GPA/COUNT/CMD/STATUS), a `BlockBase` seam keeping host I/O out of the deny-listed `dh-devices` crate, a 64 KiB-cluster overlay populated by read-modify-write, overlay-first reads, and a sorted, digest-checked dirty-cluster snapshot. The `FileBase` production backend opens `O_RDONLY` so the "base bytes + mtime never change" contract holds by construction, and there is a real acceptance test for it.

I focused on determinism (record-vs-replay divergence), partial-failure paths, integer-overflow edges in `request_range`/`restore`, the cluster-key derivation, the MMIO size-pair matching, and snapshot/restore coherence. **I found no replay-divergence hazard** — the hot paths are pure functions of (base, overlay, guest RAM, registers), and the one place HashMap iteration order could leak (snapshot) is explicitly sorted and tested. The issues I did find are about the run-control coherence contract on partial/host failures, a snapshot field omission, and a few hardening/maintainability gaps. None are replay-Critical.

The most material findings:

1. **(Important)** A partial guest-fault in `do_write` leaves the overlay populated *and* `status = STATUS_MEM_FAULT`, but a partial fault in `do_read` leaves *some* guest RAM chunks written before the fault. Both are deterministic, but the partial guest-RAM write on a `do_read` MEM_FAULT is a guest-visible side effect that the doc comment does not acknowledge — worth a deliberate spec note so it is not mistaken for a bug later.
2. **(Important)** `snapshot`/`restore` serialize `status` but **not** `host_io_errors`. After a restore of a snapshot taken when `status == STATUS_HOST_IO (0xFE)`, the device reports a host-IO STATUS with `host_io_errors == 0`. The run-control "check the counter after dispatch and fault the slot" contract reads coherently for *live* dispatch, but a restored 0xFE STATUS with a zeroed counter is an incoherent pair the doc comment implicitly promises is impossible.
3. **(Suggestion)** `request_range` computes `self.sector * SECTOR_SIZE` and `self.count as usize * SECTOR_SIZE` with unchecked `*`. They cannot overflow *given* the `end_sector <= capacity` guard on a sane `len_bytes`, but the safety argument is non-local and undocumented; a hostile `BlockBase::len_bytes` near `u64::MAX` plus the 32-bit-`usize` `restore` multiply are the only paths and both are currently unreachable on the real x86_64 target.

## Verdict

**APPROVE WITH NITS.** No Critical findings; no replay-divergence hazard. The two Important items are coherence/contract clarifications (one snapshot-field omission that is a latent correctness gap once run-control consumes `host_io_errors`, one documentation gap on partial-read side effects). Recommend addressing #2 (snapshot `host_io_errors` or assert it is transient and cleared on restore) before this device is wired into run control.

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 2 |
| Suggestions | 6 |
| Positive notes | 7 |

- Files reviewed: 4 changed (+ 4 cross-read for context)
- Tests in the change: 11 (`blk.rs` 9, `blkfile.rs` 2) — all meaningful, no tautologies found
- Replay-divergence hazards: **0**
