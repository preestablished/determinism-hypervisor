# Action Items

### Critical

_None._

### Important

- [ ] **Guard against fork-of-fork (CoW child as a fork parent).** A CoW child's
  divergent pages live in private anon memory, not in the memfd it holds (which is a
  clone of the *parent's*). `fork_slot_vm(child)` would map the parent's memfd and
  silently drop the child's divergence; `freeze_ram(child)` re-seals the parent's
  memfd and is meaningless. Add an `is_cow_child: bool` field to `SlotVm`
  (`crates/dh-vmm/src/kvm.rs:280`), set it `true` in `fork_slot_vm` and `false` in
  `create_slot_vm`, and have `fork_slot_vm` return `KvmError::Memory("fork-of-fork
  (R9): a CoW child cannot be a fork parent ...")` at entry. If deferring, the
  `fork_slot_vm` and `freeze_ram` doc comments MUST say loudly that both are
  undefined/wrong on a CoW child — today neither does. (kvm.rs:153–178, 289–325)

- [ ] **Make the device-inheritance assertion non-tautological.** In
  `crates/dh-worker/tests/fork_engine.rs`, the `bus_state(&bus_c) == bus_state(&bus_p)`
  assertions (e.g. line 436) compare default-against-default because the parent
  `test_bus()` carries no non-default device state and `PvClock::snapshot` excludes
  `vns_base`. Before freezing, drive a non-default device register on the parent bus
  (mirror `tests/restore_engine.rs:105`, which writes `CLOCK_BASE +
  REG_TIMER_DEADLINE`) so the assertion actually proves the device section rode
  through `build_dhsnap` → `apply_dhsnap` into the child.

### Suggestions

- [ ] **Cover `ForkError::Apply` and the mismatched-`child_bus` negative.** Add a
  case to `fork_preconditions_fail_loudly`
  (`crates/dh-worker/tests/fork_engine.rs:591`) that forks with a `child_bus` whose
  shape differs from the parent's (e.g. two pv-entropy devices or a missing device),
  asserting `Err(ForkError::Apply(_))`. This reaches an otherwise-unreachable
  variant and closes the missing negative test. (`Capture` remains hard to provoke;
  document it as intentionally rare or leave a comment.)

- [ ] **Preserve structured error identity across the fork boundary.** In
  `crates/dh-worker/src/fork_engine.rs:217–222`, the `RestoreError → ForkError` match
  already special-cases `Kvm`; add a `ConfigMismatch` arm (and consider carrying the
  source: `Apply(RestoreError)`) so a programmatic caller can branch on the most
  actionable cases instead of parsing a `{:?}` string.

- [ ] **Reduce `fork_slot`'s 9 positional params** (`fork_engine.rs:176–186`).
  Bundle the request into a `ForkRequest` struct (as `BoundaryState` already bundles
  scalars) to make the call site transposition-proof — `&MmioBus` parent vs
  `&mut MmioBus` child are easy to swap silently. Apply the same to
  `restore_snapshot` if refactoring.

- [ ] **Assert freezing is a ref no-op** in
  `forked_child_snapshots_to_the_parents_exact_ref`
  (`crates/dh-worker/tests/fork_engine.rs:664`): the parent is snapshotted at
  `Paused` then frozen for the fork; an explicit `snapshot(parent_paused) ==
  snapshot(parent_frozen)` would pin the assumption the ref-identity test silently
  relies on.

- [ ] **Document the child's `SlotState` ownership** on `ForkOutcome.child`
  (`fork_engine.rs:155`): state plainly that the caller MUST register the child as
  `SlotState::Paused` and that this engine does not track slot state, matching how
  `restore_engine` documents its scrap-slot contract.
