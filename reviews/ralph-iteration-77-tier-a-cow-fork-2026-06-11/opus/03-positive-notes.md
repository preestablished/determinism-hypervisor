# Positive Notes

## P1. The "ONE CODEC, TWO TRANSPORTS" thesis is genuinely realized

The two extractions (`assemble_slot_vm`, `apply_dhsnap`) are exactly the right seams: the
fork path reuses `build_dhsnap` (capture) and `apply_dhsnap` (restore) unchanged, so fork
transparency reduces to the already-proven snapshot transparency plus kernel CoW. There is
no fork-only serialization to drift — the module doc makes this argument explicitly and the
code lives up to it. This is the kind of refactor where the test surface *shrinks* the
risk rather than growing it.

## P2. The dual-guard test is the standout

`fork_preconditions_fail_loudly` proves the two guards are independent by passing a *lying*
caller (`SlotState::Frozen` over an unsealed memfd) and confirming the kernel half still
refuses. It also sweeps Paused/Running/Empty → `ParentNotFrozen` and agenda_empty=false →
`AgendaNotEmpty`. Every documented `ForkError` precondition variant is reachable — exactly
what the integration-testing research calls for (each documented error variant exercised).

## P3. The ref-identity headline test is load-bearing, not decorative

`forked_child_snapshots_to_the_parents_exact_ref` threads both the parent and the forked
child through the **real in-process snapshot-store** and asserts the 32-byte content-
addressed refs are equal. Because the ref is BLAKE3 over the materialized RAM pages plus the
DHSNAP container, this cannot pass spuriously: a single leaked byte across the CoW boundary,
or any codec drift between the fork and snapshot paths, would change the ref. It directly
underwrites the a6s fork-transparency ACCEPT.

## P4. EPT-level CoW is proven, not just host-side

`guest_writes_in_the_child_cow_and_never_reach_the_parent` actually *runs guest code* in the
child (three MOVs + HLT in real mode) so KVM faults the pages through the private mapping —
proving CoW at the EPT/host-pagetable level, then confirms the frozen parent's bytes are
still zero. Most CoW tests stop at host-side `write_slice`; this one closes the loop the
hot path actually depends on. The companion `second_child_sees_the_pristine_parent...`
proves the frozen parent stays a stable fork base after a sibling diverges.

## P5. madvise/dirty-ring inheritance is correct by sharing, not by duplication

Routing the child through `assemble_slot_vm` means `MADV_NOHUGEPAGE` lands on the child
region and the dirty ring is enabled on the child VM — both load-bearing (4 KiB CoW
granularity; future incrementals off the fork point) and both free, because the shared
assembly tail can't forget them the way a hand-rolled fork path could.

## P6. Fail-closed defaults throughout

`fork_slot_vm` rejects an unsealed parent rather than forking a mutable "snapshot";
`MFD_ALLOW_SEALING` is created up front (sealing cannot be retrofitted); the `GuestRegionMmap::new`
address-overflow case is handled explicitly. The error messages are specific and
actionable ("freeze_ram first", "CoW mapping", "CoW region address overflow"). The posture
matches the §2.1 "hard-fail loud" philosophy the rest of the codebase holds to.
