# Critical & Important Findings

No **Critical** defects found in the implemented scope.

---

## IMPORTANT 1 — Fork-of-fork silently discards the child's CoW divergence (latent correctness hazard)

**File:** `crates/dh-vmm/src/kvm.rs:153–178` (`fork_slot_vm`), with the root cause
in `SlotVm::ram_memfd` (kvm.rs:289–296).

**Problem.** A CoW child's RAM is a `MAP_PRIVATE` mapping built over a *clone of the
parent's memfd* (`FileOffset::new(memfd, 0)`, kvm.rs:167). The child's divergent
pages live in child-private **anonymous** memory created by the CoW fault — they are
**not** in the memfd. But `ram_memfd()` (kvm.rs:289) returns whatever `File` the
region's `FileOffset` holds, which for a child is the *parent's* memfd. Therefore:

- `fork_slot_vm(child)` would read `child.ram_seals()` → the **parent's** seals
  (sealed → passes), then `try_clone()` the **parent's** memfd and map *that*
  privately. The grandchild would inherit the **parent's** bytes and **silently
  drop every page the child diverged** — a corrupt grandchild that looks healthy.
- `child.freeze_ram()` would re-seal the **parent's** memfd (idempotent, harmless)
  but is semantically meaningless: it does nothing to protect the child's private
  anon pages, which are unsealable by construction. A caller who "froze the child"
  to fork it has a false sense of safety.

Nothing in code prevents this. The doc comments only say "unfreezing while children
exist is the slot manager's bookkeeping" — they never say *a child cannot be a fork
parent*. This is precisely the kind of edge that passes every current test (the
suite only ever forks the original parent) and detonates the first time the slot
manager tries a fork tree.

**Why Important, not Critical:** no current caller forks a child — `fork_slot` is
only invoked from tests on the root parent, and the slot-manager integration that
would wire fork trees does not exist yet. But the hazard is baked into the data
model (a child cannot distinguish "my memfd" from "the parent's memfd I happen to
hold"), so it must be closed *before* a multi-generation caller lands, not after.

**Fix — make `fork_slot_vm` reject a CoW child as a parent, fail-closed:**

Tag CoW slots and refuse to fork them. Minimal, self-contained:

```rust
pub struct SlotVm {
    pub vm: VmFd,
    pub vcpu: VcpuFd,
    pub guest_mem: GuestMemoryMmap<()>,
    pub mem_bytes: u64,
    /// True when guest_mem is a MAP_PRIVATE CoW mapping of ANOTHER slot's
    /// memfd (a tier-A fork child). Such a slot's divergent pages live in
    /// private anon memory, NOT in the memfd it holds — so it can never be
    /// a fork PARENT (a grandchild would map the original memfd and silently
    /// drop this slot's divergence) and freeze_ram on it is meaningless.
    is_cow_child: bool,
}
```

Set `is_cow_child: true` in `fork_slot_vm`'s `SlotVm` construction (thread it
through `assemble_slot_vm`, or set it on the returned value), `false` in
`create_slot_vm`. Then gate the fork:

```rust
pub fn fork_slot_vm(&self, parent: &SlotVm) -> Result<SlotVm, KvmError> {
    if parent.is_cow_child {
        return Err(KvmError::Memory(
            "fork-of-fork (R9): a CoW child cannot be a fork parent — its \
             divergent pages live in private anon memory, not the memfd; \
             snapshot it and fork from a fresh frozen slot instead".into(),
        ));
    }
    let seals = parent.ram_seals()?;
    // ...
}
```

If a fork tree is genuinely wanted later, that is a deliberate design extension
(materialize the child to its own memfd first), not something to fall into
silently. At minimum, if the team prefers not to add the field now, the
`fork_slot_vm` *and* `freeze_ram` doc comments must state loudly that they are
**undefined / wrong on a CoW child** — today neither does.

---

## IMPORTANT 2 — The transparency tests do not prove device-state inheritance (tautological assertion)

**File:** `crates/dh-worker/tests/fork_engine.rs:436` (and the `frozen_parent`
helper, fork_engine.rs:386–401).

**Problem.** Three tests assert `bus_state(&bus_c) == bus_state(&bus_p)` to claim
"the child IS the parent's machine." But:

1. `bus_state` snapshots each device. For the clock, `PvClock::snapshot` serializes
   **only** `timer_deadline_vns` + `timer_vector` (clock.rs:165–168) — it
   deliberately **excludes `vns_base`**. So whatever `apply_dhsnap` sets on the
   child (`set_vns_base`, restore_engine.rs:335) is *invisible* to `bus_state`.
2. The parent bus in every fork test is a **fresh `test_bus()`** with **no MMIO
   writes** — every device carries its default snapshot. `frozen_parent` writes
   RAM and vCPU regs but never touches a device register.

So `bus_state(&bus_c) == bus_state(&bus_p)` compares **child-defaults restored from
the parent's defaults** against **the parent's defaults** — it passes for any
machine, including one where device inheritance is completely broken, as long as
`apply_dhsnap` writes the same defaults. It proves nothing about non-default device
state surviving the fork.

The restore-engine suite already does this correctly: `tests/restore_engine.rs:105`
writes `CLOCK_BASE + REG_TIMER_DEADLINE` (a **non-default** device state) before
asserting `bus_state(&bus_b) == bus_state(&bus_a)` at line 164. The fork suite
omits exactly that step, leaving the fork's device path asserted only against
defaults. This is the "tautological test" pitfall the project's own
`~/.claude/research/rust-integration-testing.md` flags.

**Fix.** In `frozen_parent` (or at least in
`fork_inherits_the_exact_machine_and_cow_isolates_host_writes`), drive a
non-default device register on the parent bus before forking, mirroring the
restore test:

```rust
// In frozen_parent, after building `bus`, before freeze:
let ctx = /* the same write ctx restore_engine.rs uses */;
bus.write(CLOCK_BASE + REG_TIMER_DEADLINE,
          &0xDEAD_BEEFu64.to_le_bytes(), ctx).unwrap();
```

Then `bus_state(&bus_c) == bus_state(&bus_p)` actually proves the deadline rode
through `build_dhsnap` → `apply_dhsnap` into the child — and would fail if the
device section were dropped or reordered. Without this, the central "fork is the
parent's machine" claim is under-tested at the device layer.
