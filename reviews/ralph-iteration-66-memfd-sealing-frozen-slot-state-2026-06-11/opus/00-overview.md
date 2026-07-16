# Review Overview — memfd sealing + Frozen slot-state machine

- **Branch:** `ralph/iteration-66-memfd-sealing-frozen-slot-state` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-d2p (risk R9 — fork-parent corruption)
- **Commit:** `ffb8023` — ralph iteration 66 checkpoint

## Summary

This change lands both halves of the R9 fork-parent guard. The **software half**
(`lib.rs`) adds a `SlotState` transition relation (`can_transition` / `transition`)
and `ensure_write_path`, which loudly denies write-path APIs on `Frozen` (and
`Empty`) slots via a typed `SlotStateError`. The **hardware half** (`kvm.rs`) adds
`SlotVm::freeze_ram`, applying `F_SEAL_FUTURE_WRITE | F_SEAL_SHRINK | F_SEAL_GROW`
to the guest-RAM memfd (deliberately omitting `F_SEAL_WRITE`, which is unavailable
while the parent's KVM mapping lives, and `F_SEAL_SEAL`, to keep re-freezing
idempotent), plus a `ram_seals()` probe. The transition relation matches ARCH §8.4
and API §2.2 exactly (`Empty→Paused`, `Paused⇄Running`, `Paused→Frozen`,
`Frozen→Paused`, `Paused→Empty`, `Frozen→Empty`; all self-transitions and
`Running→{Empty,Frozen}` rejected). The work is rigorously tested: a full 4×4
transition matrix, a fork-lifecycle walk, write-path denial assertions, and a
live KVM test that proves the seal lands, a new writable `mmap` returns `EPERM`,
a read-only mapping survives, the existing mapping stays writable, and truncation
is sealed. The guard is **not yet wired into any engine caller** (beads 9e4 / qmp /
ol1) — correctly out of scope, but the unused-rot risk deserves an explicit tracking
note. The omission of a `Faulted` variant from `lib.rs SlotState` (the proto has
`FAULTED_S`) is correct scoping for this bead but should be acknowledged so it is
not silently lost.

## Verdict

**APPROVE**

The change is correct, spec-faithful, well-tested (host + live KVM), and minimal.
All findings are non-blocking (suggestions / tracking notes). No Critical or
Important issues.

## Stats

- Files changed: 2 (`crates/dh-vmm/src/kvm.rs`, `crates/dh-vmm/src/lib.rs`)
- New public API: `SlotVm::freeze_ram`, `SlotVm::ram_seals`, `SlotState::can_transition`,
  `SlotState::transition`, `SlotState::ensure_write_path`, `SlotStateError`
- New tests: 5 host-runnable (`slot_state_tests`) + 1 live KVM (`freeze_ram_*`)
- `cargo test -p dh-vmm --lib`: **80 passed; 0 failed** (live KVM tests ran, `/dev/kvm` present)
- `cargo clippy -p dh-vmm --lib`: **clean** (no warnings/errors)
- New `#[allow(unsafe_code)]` sites: 5 (fcntl ×2 in `freeze_ram`/`ram_seals`; mmap ×2 + munmap ×1 in the live test)
- External callers of the new guard: **0** (unwired — expected, tracked below)
