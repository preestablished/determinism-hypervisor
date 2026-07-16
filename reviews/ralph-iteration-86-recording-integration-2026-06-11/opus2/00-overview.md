# Iteration 86 — Recording Integration (bead y78) — 2nd-Reviewer Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-86-recording-integration`
- **Bead:** determinism-hypervisor-y78 (P0, IN_PROGRESS)
- **Diff:** 5 files, +582 (`/tmp/iter86.diff`)

## Scope reviewed

- `crates/dh-vmm/src/recording.rs` (new, 548 lines) — every method, both `mod tests` and `mod live_tests`, read in full.
- Verified against: `crates/dh-devices/src/{pad.rs,net.rs}`, `crates/dh-inputlog/src/{reader.rs,dhilog.rs}`, `crates/dh-vmm/src/{runctl.rs,boundary.rs}`, `crates/dh-devices/src/{lib.rs,bus.rs}`.
- Cross-pin in `crates/dh-worker/src/proto_map.rs`.
- Beads `y78`, `a5e`; research `~/.claude/research/rust-integration-testing.md`.

## Summary

This is a clean, well-documented productization of the m1_acceptance on_exit harness into a reusable `DeviceRail<M>`. The pairing of device mutation with record landing in `apply_pad_set`/`apply_net_rx` is the right shape (an applied-but-unrecorded input is a divergence by construction, and the code makes that impossible). The seal-from-outcome path, the undrained-IRQ-queue guard, and the `stop_reason_u8` proto cross-pin are all correct, and the live 3-segment pad_echo proof genuinely exercises the guest-visible latch-flip + FRAME_MARK + canonical PAD_SET record stream end-to-end. Every API surface the tests touch (`rec.icount()`, `RecordBody::{PadSet,NetRx,FrameMark}`, `r.{canonical,aux,end}()`, `Boundary{icount,rip,rcx}`, `SegmentOutcome` fields, `VcpuExit::MmioWrite(u64,&[u8])`) was verified to exist with the asserted shape.

**One real correctness bug** sits in `drain_net_tx`: it reads `tx_regs()` (last-*programmed*, uncapped, **status-unchecked** registers) and will mint a loopback frame for a TX whose doorbell *faulted* — minting a NET_RX with no corresponding NET_TX record, plus a `vec![0u8; len as usize]` allocation driven by an unvalidated guest `u32` (up to 4 GiB). It is gated behind a not-yet-wired loopback caller, so it cannot fire today, but it is a latent replay-divergence + DoS that must be fixed before the loopback path (czq) or a5e's sibling work goes live.

**One critical-path discovery for a5e:** no EPOCH_HASH *writer* exists anywhere in the tree. `LogWriter` has no `epoch_hash()` method; only the kind constant and the reader-side `RecordBody::EpochHash` exist. a5e's acceptance ("every EPOCH_HASH equal") cannot start until something logs EPOCH_HASH records into the DHILOG during a recording run, and no bead currently owns that producer. This is the next gap on the a5e critical path.

## Verdict

**APPROVE WITH NITS** — y78's own deliverable is sound and the tests prove it. Ship it. But two follow-ups must be filed before they bite: (1) fix `drain_net_tx` to gate on `STATUS_OK` and cap `len`, and (2) file the missing EPOCH_HASH-writer bead. Neither blocks merging y78 (drain has no live caller; epoch-hash is out of y78's stated scope), but both are on the a5e critical path.

## Stats

| Class | Count |
|---|---|
| Critical | 1 (`drain_net_tx` status/cap — latent, no live caller yet) |
| Important | 2 (missing EPOCH_HASH writer for a5e; doc/test "segment" terminology drift) |
| Suggestions | 5 |
| Positive notes | 7 |

## Adversarial angles run (all 7)

1. drain_net_tx faulted-doorbell frame minting — **CONFIRMED BUG** (Critical).
2. Doc "build fresh per segment" vs test reusing one rail across 3 segments — **real terminology drift**, doc is imprecise; test is correct for M5 (Important).
3. apply_* icount/rip from outcome vs the `0` rip convention in `service_exit` — **asymmetry is fine and intentional** (suggestion to document).
4. Vector path end-to-end (IRQ_VECTOR set → apply → queued → ScheduledInjection → delivered) — **test gap, but out of y78's scope** (suggestion).
5. `mod tests` on non-x86 / redundant inner cfg — **redundant-but-harmless**, recording.rs is x86-gated at lib.rs (suggestion).
6. `VcpuExit::MmioWrite(addr,&data)` synthetic construction — **safe public-API enum construction, no UB** (positive note).
7. EPOCH_HASH writer existence for a5e — **no writer exists** (Important / critical-path gap).
