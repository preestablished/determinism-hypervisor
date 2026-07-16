# Critical and Important Issues

No **Critical** issues. Two **Important** issues, both about contract/doc
honesty rather than a wrong result on the tested path.

---

## Important 1 — The doc's load-bearing EVTC justification references a device this engine cannot drive

**Files:**
- `crates/dh-worker/src/restore_engine.rs:7-9` (module doc)
- `crates/dh-worker/src/restore_engine.rs:84` (inline ORDER comment)
- `crates/dh-devices/src/detchannel.rs:96,123,219` (the type and its `restore`)

**Problem.** The module doc states the ordering is load-bearing *specifically*
because EVTC re-attach reads live RAM:

```rust
//! ORDER IS LOAD-BEARING: device restore may validate against live guest
//! RAM (DetChannelHost's EVTC re-attach reads the channel region — see
//! detchannel.rs), so RAM populates before any `DetDevice::restore` runs.
```

But `DetChannelHost` is `struct DetChannelHost<M: GuestMem + Clone, P: FaultPlan>`
and its restore is an **inherent** method with a different signature:

```rust
pub fn restore(&mut self, bytes: &[u8], version: u16, plan: P) -> Result<(), crate::RestoreError>
```

It does **not** implement `DetDevice` anywhere in the crate (grep confirms no
`impl ... DetDevice for DetChannelHost`), and being generic over `M`/`P` it
cannot be coerced to `Box<dyn DetDevice>`. Therefore EVTC cannot currently be
registered on `MmioBus`, and `bus.devices_mut()` will never yield it. The one
device the doc cites as the *reason* for RAM-first ordering is unreachable
through this engine. A future maintainer who removes/changes the ordering on
the basis "no on-bus device reads RAM today" would be correct about the code
but contradicting the doc — exactly the kind of silent drift that bites later.

Note this is a doc-vs-reality gap, not a runtime bug: RAM-first ordering is
still the right call (it matches ARCH §8.3 and is the obvious invariant for any
*future* RAM-reading device), and PvBlk/PvEntropy/etc. restore correctly
regardless of order because each looks up its own section by tag.

**Suggested fix.** Soften the justification to "future-proofing" and stop
implying EVTC is on-bus today, e.g.:

```rust
//! ORDER IS LOAD-BEARING: a device restore MAY validate against live guest
//! RAM (the intended example is DetChannelHost's EVTC re-attach, which reads
//! the channel region — though EVTC is not yet a `DetDevice` and so is not
//! reachable through this engine). RAM is therefore populated before any
//! `DetDevice::restore` runs so the invariant holds the moment such a device
//! lands. The vCPU goes last; ...
```

If EVTC is meant to be on-bus this iteration, that is a larger gap: a
`DetDevice` impl (and a `DevCtx`/plan-supplying restore seam) would be required,
and `tag_for_device_id(0x0001) => EVTC` (dhsnap.rs:85) currently has no producer.

---

## Important 2 — `restore_snapshot`'s "Paused is sufficient" contract under-specifies slot reuse; dirty-ring / KVM dirty-log state is not reset

**File:** `crates/dh-worker/src/restore_engine.rs:88-105, 277-284`

**Problem.** The only precondition the engine enforces is `slot_state ==
SlotState::Paused` (line 103). The doc says "the slot holds exactly the
snapshot's state" on success. But the engine only overwrites:
- guest RAM pages that the snapshot covers (full coverage is enforced — good),
- vCPU state (full `KVM_SET_*` — good),
- the device-model fields of each on-bus `DetDevice`,
- the caller's `DirtyPageSet` (cleared, line 282-283).

It does **not** drain/reset the KVM **dirty ring** or `KVM_CLEAR_DIRTY_LOG`
state. If a caller restores into a slot that previously *ran* with dirty
logging enabled (a `Paused` slot can be a previously-Running one per the §9
lifecycle `Created → Paused ⇄ Running`), stale ring entries survive the
restore. The engine writes every RAM page via `write_slice` through the
KVM-registered mapping, which (with logging on) also re-dirties pages in the
ring. The next *incremental* `take_snapshot` would then harvest a ring that
mixes pre-restore staleness with the engine's own restore writes — a wrong
delta. The current tests never hit this because every restore target is a
*freshly created* slot (`sys.create_slot_vm(...)`), so the latent hazard is
invisible.

This is "important" rather than "critical" only because every present caller
uses a fresh slot; it is a correctness landmine the moment slot reuse is added.

**Suggested fix.** Either tighten the contract or harden the engine. Minimal
contract tightening (cheap, honest):

```rust
/// PRECONDITION: `slot` must be FRESH — created and never run since
/// creation (no dirty-ring entries, dirty logging not yet enabled).
/// Restoring into a slot that previously ran is undefined: this engine
/// resets the caller's DirtyPageSet but NOT KVM's dirty ring / log state.
```

Better, if slot reuse is a goal: accept the `&mut DirtyRing` (as
`take_snapshot`'s incremental path already does) and drain+discard it before
clearing the `DirtyPageSet`, or assert the ring is empty. A test that restores
into a slot that has executed guest code and then takes an *incremental*
snapshot would lock this down.
