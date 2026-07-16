# Iteration 82 — ENTR golden (bead dy8) — Review Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-82-entr-golden`
- **Scope:** ~531 diff lines — `tests/nanokernel/asm/entropy_draw.asm` (new guest),
  `crates/dh-worker/tests/entr_golden.rs` (new acceptance), nanokernel wiring
  (`lib.rs`, `build.rs`) and drift pins (`elf_shape.rs`).

## Summary

This iteration adds the M4 ENTR-golden acceptance: a new `entropy_draw` guest that
pulls 16-byte fills through the **real** pv-entropy MMIO doorbell, rings the output at
`0x50_0000`, and HLTs at every 256-draw batch boundary. The harness snapshots mid-stream
(after 512 draws), restores into a fresh slot, and proves the restored machine's next
1024 draws are **byte-identical** to the un-snapshotted continuation, with a final
`DetEntropy::state()` equality clinching the {seed, stream, word_pos} tuple round-trip.

I verified the load-bearing claims against source rather than the summary:

- **HLT resume is correct and load-bearing.** `run_segment` classifies `VcpuExit::Hlt`
  as `halted=true` (runctl.rs:241) and `finish_halted` reports `StopReason::GuestHalted`
  with the real `counter.read()` icount and `get_regs().rip` (runctl.rs:418-444). On KVM,
  the `hlt` is emulated with RIP advanced **past** the instruction before the userspace
  exit, so the next `run_segment` (next batch) resumes at `jmp .batch`, not by re-executing
  `hlt`. This is the *novel* re-entry pattern — see the IMPORTANT note: no prior test
  re-enters after a HLT (pad_echo loops without halting; the `terminal_hlt` live test halts
  exactly once and stops). The design is sound but the round-trip rests entirely on this
  newly-exercised behavior.

- **Register survival across snapshot/restore is real.** `vcpu_state::capture` stores the
  whole `kvm_regs` via `vcpu.get_regs()` (vcpu_state.rs:126) and `restore` does
  `set_regs(&st.regs)` (vcpu_state.rs:155); `encode_section` serializes the full 144-byte
  struct via `struct_bytes(&st.regs)` (vcpu_state.rs:244). r8 (ENT_BASE) and r9 (TABLE_GPA)
  — live at the snapshot boundary (post-HLT of batch 2, RIP at `jmp .batch`) — therefore
  round-trip exactly. That is *why* leg B can keep drawing: it inherits leg A's r8/r9/RIP
  and the device window is re-registered by `test_bus()`.

- **GPA math checks out.** `shl rdx,4` is ×16 (DRAW_BYTES); the ring is
  `8 + 2^15·16 = 0x580008`, matching the asm comment. The ring `[0x50_0000, 0x58_0008)`
  clears the guest image at `0x10_0000`, timer table `0x20_0000`, pad_echo `0x30_0000`,
  and channel `0x40_0000` with no collision, and sits inside the 16 MiB (`0x100_0000`)
  guest RAM.

- **The attestation is honest.** `take_snapshot` *trusts* the caller's `BoundaryState`;
  the slot is at a real instruction boundary (RIP at `jmp .batch`, IF=0), the boundary
  engine owns the Paused transition for a GuestHalted segment, and icount/vns/hash_chain
  come from the actual `a1` outcome.

## Verdict

**ACCEPT (with one IMPORTANT note and a few suggestions).** The acceptance is well-built,
the determinism claim is genuinely tested end-to-end against the real device path, and the
batch-boundary design correctly side-steps the landing/MMIO hazard (filed as bead 4a3).
No correctness defects found. The standout gap is **test coverage of the very thing the
design newly relies on** (HLT→resume across segments) and the un-asserted fault path /
`cumulative_icount`, all addressable as follow-ups, none blocking.

## Stats

| Metric | Count |
|---|---|
| Files reviewed (full) | 2 (asm + acceptance) + 3 wiring/drift |
| Critical findings | 0 |
| Important findings | 1 |
| Suggestions | 6 |
| Positive notes | 8 |
| Source files cross-checked | 7 (runctl, boundary, vcpu_state, entropy, ctx, snapshot/restore engine, common) |
