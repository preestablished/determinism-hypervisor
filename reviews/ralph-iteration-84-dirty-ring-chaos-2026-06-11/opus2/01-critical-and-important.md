# Critical & Important

## Critical

None.

## Important

### I1 — `map_sized` / `SlotVm::dirty_ring_entries` are decoupled; nothing enforces the docstring's "MUST match"

`DirtyRing::map_sized(vcpu, entries)` takes a raw `&VcpuFd` and a free `entries: u64`. Its docstring (dirty.rs:65–68) and the new `SlotVm::dirty_ring_entries` field doc (kvm.rs:330–333) both warn that `entries` MUST equal the size the VM enabled the ring with — "a mismatch would mis-mask the free-running cursor." But the type system does not tie them together. In `ring_chaos.rs` the caller threads the SAME local `ring_entries` into both `create_slot_vm_with_ring(MEM, ring_entries)` (kvm.rs:235 `cap.args[0] = ring_entries * 16`) and `DirtyRing::map_sized(&slot.vcpu, ring_entries)` (ring_chaos.rs:70 + :90) — correct *only because the test happens to use one variable*.

The failure mode if they ever drift is genuinely nasty and silent-ish:
- `map_sized` too small → mmap is shorter than the kernel's ring → `harvest_into`'s `% self.entries` masks the cursor into a sub-window; entries the kernel published past `entries` are never read → **lost dirty pages**, exactly the R8 failure this whole iteration exists to disprove. No panic, no error — just a wrong (smaller) delta.
- `map_sized` too large → reads/writes past the real ring mapping → out-of-bounds on the mmap.

And the field added this iteration — `SlotVm::dirty_ring_entries` — is the authoritative source but is **never actually consumed** by a `map` call anywhere. It exists only to be documented. That's the smell: the safe value lives on the slot, yet the unsafe `map_sized(vcpu, raw)` path is the one the test uses.

**Minimal hardening (low churn, recommended):** add a slot-keyed constructor and route callers through it, so the entry count is never re-typed at the map site:

```rust
impl DirtyRing {
    /// Map the slot's ring at the size the VM was created with — the
    /// cursor mask cannot drift from `cap.args[0]`.
    pub fn map(slot: &SlotVm) -> Result<Self, KvmError> {
        Self::map_sized(&slot.vcpu, slot.dirty_ring_entries)
    }
}
```

This changes `map`'s signature from `&VcpuFd` to `&SlotVm` (the existing default-ring callers in `dirty.rs` live_tests and the snapshot_engine/restore tests pass a `slot` anyway, so they become `DirtyRing::map(&slot)` — a one-token change each). `map_sized` can then drop to `pub(crate)` or stay `pub` for the rare explicit-size case, but the test would use `DirtyRing::map(&slot)` and the `dirty_ring_entries` field becomes load-bearing instead of documentation-only.

I weighed the alternative (leave it as-is, doc-only): given that THIS iteration's entire thesis is "no dirty page is lost," shipping a map path whose only guard against silently losing pages is "the test author used one variable" is the weakest link in an otherwise airtight test. The fix is a handful of call-site token changes. Worth it. If churn is truly unwanted this cycle, file it — but it's Important, not cosmetic.

### I2 — `0vl` should carry an explicit `bd dep` blocking `9sb`, not just prose

`determinism-hypervisor-0vl` names the 9sb impact in its description ("the perf acceptance plans a 128MiB guest — it will hit this") but there is no dependency edge. `9sb` is OPEN, P1, and its acceptance is a 128MiB guest FULL/incremental/restore perf gate — which will FULL-snapshot 128MiB and hit the exact ep_poll hang 0vl documents at 32MiB. Today 9sb's `DEPENDS ON` lists only the closed 9e4/9wa; nothing stops someone claiming 9sb and burning a session rediscovering the hang.

**Action:** `bd dep add determinism-hypervisor-9sb determinism-hypervisor-0vl` (9sb is blocked until 0vl closes). This is the honest graph: 9sb is not "ready" until the blocking-client put path stops hanging on large FULL snapshots. The prose note is good but `bd ready` / `bd graph` don't read prose.
