# Iteration 77 — Tier-A CoW Fork — Second-Reviewer Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-77-tier-a-cow-fork`
- **Bead:** 9e4 (ARCH §8.4)
- **Scope:** `dh-vmm/src/kvm.rs` (`fork_slot_vm`, `assemble_slot_vm` extraction),
  `dh-worker/src/fork_engine.rs` (new), `dh-worker/src/restore_engine.rs`
  (`apply_dhsnap` extraction), `dh-worker/src/snapshot_engine.rs` (`build_dhsnap`
  visibility), `dh-worker/tests/fork_engine.rs` (new, 5 live tests). ~735 diff lines.

## Summary

The change is well-built and the architectural spine is sound: one codec, two
transports (`build_dhsnap` → `apply_dhsnap`), with RAM delivered over CoW instead
of the store. I verified the load-bearing implicit assumptions and they hold:

- **`assemble_slot_vm` order preserved** — dirty-ring cap → USER_SPACE_MSR →
  `madvise_nohugepage` → memslot → vPMU-off → vCPU → CPUID, identical to the
  pre-refactor sequence (kvm.rs:193–261). `madvise_nohugepage` **does** run for
  the fork path — it lives in the shared assembler, called for both
  `create_slot_vm` and `fork_slot_vm` (kvm.rs:224). Verified against the final
  code, not the description.
- **Dirty ring on the child** — enabled in `assemble_slot_vm` (kvm.rs:196–204),
  so the child gets a fresh ring for later incrementals. Present.
- **`MmapRegion::build`** maps with exactly the passed `PROT_READ|PROT_WRITE` and
  `MAP_PRIVATE|MAP_NORESERVE` (vm-memory 0.18 `mmap/unix.rs:121`), forbids only
  `MAP_FIXED`. The cloned `File` is owned by the `FileOffset` stored inside the
  `MmapRegion`, so the memfd lives as long as the mapping. Correct.
- **`GuestRegionMmap::new`** checks only `guest_base + size` overflow
  (mmap/mod.rs:79), no page-alignment check — irrelevant here (base 0,
  page-multiple size). Fine.
- **`libc::F_SEAL_FUTURE_WRITE`** = `0x10`, `c_int` (i32), present in
  libc 0.2.186 `linux_like/linux/mod.rs:1427`. Matches the `i32` `ram_seals()`
  return. Fail-closed seal check is correct.
- **`try_clone` seal semantics** — seals are inode/file-description level, so the
  cloned fd carries the parent's seals; `ram_seals` on the child reads them. Good.

The fail-closed seal check, the two independent guards (state-machine Frozen +
kernel seal), and the CoW host/guest isolation tests are genuinely strong. My
findings are about **edges the happy path hides**: a latent fork-of-fork
correctness hazard, a tautological device-inheritance assertion, and two
unreachable error variants — exactly the pitfalls the project's own integration-
testing research file calls out.

## Verdict

**Approve with nits.** No blocking correctness defect in the implemented scope.
One Important latent-hazard item (fork-of-fork silently drops divergence — must
be guarded or loudly documented before a slot manager can call this), and a
cluster of test-quality gaps that weaken the transparency claim the bead rests on.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 2     |
| Suggestions| 5     |
| Positive notes | 6 |
