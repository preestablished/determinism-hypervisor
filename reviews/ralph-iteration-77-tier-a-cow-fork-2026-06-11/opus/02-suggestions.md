# Suggestions (non-blocking)

## S1. MAP_NORESERVE: document the OOM-on-CoW-touch posture somewhere durable

`kvm.rs:170` maps the child with `MAP_NORESERVE`, so commit is deferred to first write
fault. Under memory pressure a CoW fault can deliver `SIGBUS`/OOM-kill *during guest
execution* rather than at fork time — i.e. the fork "succeeds" and the failure surfaces
later, asymmetrically from the tier-B path. This is the right trade for the <10 ms target
(reserving 128 MiB per child up front would defeat the point), but the consequence
deserves a one-line note in the `fork_slot_vm` doc or a bead so the slot manager's
admission control accounts for it. Right now the implication lives only in this review.

**Suggested:** add to `kvm.rs:168` doc — "MAP_NORESERVE: commit is deferred; a CoW write
fault under memory pressure can SIGBUS the child mid-run. Slot admission (bead ol1) owns
the per-host child-count cap."

## S2. fork-of-fork is silently permitted but untested — pin the intent with a bead

Because `MmapRegion::build` retains the `FileOffset`, a child's `guest_mem` *does* expose a
backing memfd (the parent's clone, same sealed inode), so `fork_slot_vm(child)` would pass
the seal check and produce a grandchild whose CoW pages chain off the child's *private*
mapping. Whether that is sound (the child's own CoW writes are private anonymous pages, not
in the memfd, so a grandchild would see the parent's baseline, not the child's divergence)
is subtle and currently unspecified. Either it is intended (and wants a test) or it should
be denied (the child was never frozen via `freeze_ram`, so in practice it would fail the
`SlotState::Frozen` engine guard at the `fork_slot` layer — but `fork_slot_vm` alone would
not). File a bead to decide and pin it; do not leave the behavior emergent.

## S3. ForkError → gRPC status mapping is not yet exercised

`ForkError` has five variants but no `From`/status mapping lives in this change (consistent
with `RestoreError`, whose mapping is presumably the slot-manager bead's job). Worth a bead
note that `AgendaNotEmpty`, `ParentNotFrozen`, and the `Kvm`/`Apply`/`Capture` classes need
distinct gRPC codes (precondition-failed vs internal) when ol1 wires the RPC, so a caller
can distinguish "retry at a boundary" from "this child is scrap."

## S4. The unsealed-parent error string is matched by substring in the test

`fork_engine.rs` test asserts `m.contains("UNFROZEN")` against the `ForkError::Kvm` message
that originates as `KvmError::Memory("fork of an UNFROZEN parent ...")` formatted through
`{e:?}`. This couples a test to a human-readable string. Low risk (the string is
load-bearing documentation and unlikely to churn), but a dedicated `KvmError` variant (e.g.
`ParentNotSealed`) matched structurally would be more robust than a substring grep. Minor.

## S5. `child_bus` is caller-supplied and assumed pristine

`fork_slot` takes `child_bus: &mut dh_devices::MmioBus` and stuffs it via `apply_dhsnap`.
The shape checks inside `apply_dhsnap` (exactly one entropy device, section count equality)
will catch a wrong-shaped bus, but a caller passing a *non-fresh* child bus (one already
holding device state) relies entirely on `restore`'s wholesale-overwrite semantics. That is
the same contract `restore_snapshot` already has, so this is not new — but a one-line note
on `fork_slot` that `child_bus` must be a freshly-built bus matching the parent's machine
shape would mirror the "slot must be FRESH" precondition `restore_snapshot` documents at
`restore_engine.rs:105-110`.
