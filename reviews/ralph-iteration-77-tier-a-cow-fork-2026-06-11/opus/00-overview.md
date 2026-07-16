# Review: Tier-A CoW Fork (the hot path)

- **Branch:** `ralph/iteration-77-tier-a-cow-fork`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** 9e4 — ARCH §8.4 tier-A same-worker CoW fork
- **Scope:** ~735 diff lines, 1 commit

## Summary

This change lands the tier-A CoW fork — the milestone-1 hot path. A child slot gets
its RAM as a `MAP_PRIVATE | MAP_NORESERVE` mapping of the frozen parent's `try_clone`'d
memfd (CoW at 4 KiB), fresh `VmFd`/vCPU/EPT (no shared kernel state, R9), and its
vCPU + device state stuffed from the parent's in-memory DHSNAP through the *same* codec
a tier-B restore consumes. The "ONE CODEC, TWO TRANSPORTS" thesis is realized cleanly by
two extractions:

1. `KvmSystem::assemble_slot_vm` — the post-RAM VM assembly (dirty ring → USER_SPACE_MSR
   → madvise → memslot → vPMU-off → vCPU → CPUID mask), shared by `create_slot_vm` (shared
   memfd mapping) and `fork_slot_vm` (private CoW mapping).
2. `restore_engine::apply_dhsnap` — steps 3–6 of §8.3 (decode + devices + vCPU + reseed),
   shared by `restore_snapshot` (RAM materialized from store) and `fork_slot` (RAM CoW-shared).

Both extractions are behavior-preserving. The dual-guard story (engine checks
`SlotState::Frozen`; kernel checks `F_SEAL_FUTURE_WRITE`) is coherent and the docs are
honest that the parent-write guard is software-enforced, not kernel-enforced (R9).

I verified the refactor against `HEAD~1`: the moved `apply_dhsnap` body is character-for-
character the prior inline restore body, with only the terminal struct changed
(`RestoreOutcome` → `AppliedMachine`, `pages_loaded` now set by the caller). Error
precedence, dirty/counter handling, and section ordering are unchanged.

## Verification performed

- `cargo build -p dh-worker -p dh-vmm` — clean.
- `cargo clippy -p dh-worker -p dh-vmm --tests` — zero warnings.
- `cargo test -p dh-worker --test fork_engine` — **5/5 pass** (live KVM on this box).
- `cargo test -p dh-worker --test restore_engine` — **5/5 pass** (no regression).
- `cargo test -p dh-vmm --lib kvm::` — **7/7 pass** (freeze-seal semantics intact).
- Confirmed `vm_memory::MmapRegion::build(file_offset, size, prot, flags)` signature and
  that the `FileOffset` retains the cloned `File`, keeping the child's backing fd alive
  for the mapping's lifetime.
- Confirmed the §8.4 doc and the `SlotState`/`freeze_ram` doc comments are honest about
  the software-vs-kernel guard split.

## Verdict

**APPROVE**

The implementation is sound, the refactors are faithful, the tests are live and
load-bearing (the ref-identity test threads the real in-process store and cannot pass
spuriously), and the design thesis is honored. The findings below are all suggestions and
follow-up bead notes — none block the merge.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 0     |
| Suggestions| 5     |
| Positive notes | 6 |
