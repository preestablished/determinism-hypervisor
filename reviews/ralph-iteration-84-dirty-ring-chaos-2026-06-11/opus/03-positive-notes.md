# Positive notes

### P1 — The plumbing is minimal and surgical

The parameterization threads a single `ring_entries` value from
`create_slot_vm_with_ring` → `assemble_slot_vm` → the `enable_cap` args → the
`SlotVm` field, and a parallel `entries` through `DirtyRing::map_sized`. `create_slot_vm`
and `map` become one-line delegations to the new sized variants with `DIRTY_RING_ENTRIES`,
so the production call sites are provably unchanged in behaviour. No churn in the hot
path, no new allocations, no API removed. This is exactly the right shape for a
test-only capability.

### P2 — Power-of-two validation fails closed, early, with a useful message

`create_slot_vm_with_ring` rejects a non-power-of-two ring *before* allocating the memfd
or touching KVM, with a message that echoes the offending value. The kernel requires a
power-of-two ring; catching it here (rather than as an opaque EINVAL from `enable_cap`)
is the friendlier failure.

### P3 — The non-vacuity guard is the right instinct and well-explained

Asserting `large.ring_full_exits == 0` *and* `small.ring_full_exits >= 2` (with a message
that explains "not a chaos run") prevents the classic false-green where the stressor
silently stopped firing — the test would pass trivially if both rings were large enough to
never overflow. Pinning both ends of the overflow behaviour is what makes the equal-ref
assertion meaningful. (S3 suggests tightening the bound further, but the instinct is
exactly right.)

### P4 — The 512→1024 empirical was discovered, understood, and recorded honestly

Rather than silently bumping the constant, the author traced the EINVAL to the kernel's
64 + 512-PML reserved-entry floor and wrote it down in the test preamble with the
mechanism. Likewise the 32 MiB store hang was not papered over — it was reduced to a
clean 16 MiB repro and filed as bead 0vl (P1, BUG) with a concrete hypothesis (tonic
max-message / h2 flow-control in the put path) and a forward-looking note that the perf
acceptance (9sb) will hit it. This is good engineering discipline; the only gap is
propagating the 1024 number back into the canonical docs/bead (see I1).

### P5 — `harvest_at_boundary` is reused for the full-exit path, not reimplemented

The test services `DirtyRingFull` with the same `harvest_at_boundary` used at pause
boundaries, exactly matching the function's documented dual role (dirty.rs:204–205). No
bespoke ring-full handler that could drift from the boundary semantics — one code path,
two triggers.

### P6 — The acceptance assertion chain is genuinely strong

Identical root refs (sanity) → identical delta refs (the core property) → identical
`pages_shipped` → `pages_shipped >= PAGE_DIRTIER_PAGES` (floor) → bit-equal vCPU. Because
`SnapshotRef` is a BLAKE3 digest over the manifest body *including* the DHSNAP device
blob, the delta-ref equality alone is a content-address proof that no dirty page was lost
or reordered and that the guest's vCPU state was unperturbed by the extra exits. That is a
stronger discharge of R8 ("ring-full loses no dirty page") than the aggregate hash-chain
the plan implied. (I3 only asks that this strength be stated, not changed.)
