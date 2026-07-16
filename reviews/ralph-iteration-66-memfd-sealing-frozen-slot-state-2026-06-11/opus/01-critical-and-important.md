# Critical & Important Findings

**None.**

No Critical or Important issues were found. The transition relation is spec-faithful,
the seal choice is correct, the unsafe usage is minimal and sound, error handling is
loud and well-formed, and the live test genuinely proves the four R9 properties. All
of `cargo test -p dh-vmm --lib` (80 passed, live KVM included) and `cargo clippy`
(clean) pass on this box.

The items below are deliberate scope decisions I examined closely and concluded are
**correct** — recorded here so the reasoning is on the record, not because they need
changing.

## Examined and cleared

### C-A. `lib.rs SlotState` has no `Faulted` variant (proto has `FAULTED_S`) — correctly out of scope

`crates/dh-vmm/src/lib.rs:34-39` defines `SlotState { Empty, Running, Paused, Frozen }`.
The proto (API §2.8) `enum SlotState` carries `FAULTED_S = 5`, and `StopReason::FAULTED`
(API §2.4) exists with the note "slot needs Destroy/Restore". So `Faulted` is a real
slot lifecycle state that this Rust enum does not model.

**Position: this is correct scoping for bead d2p, not a gap to fix here.** d2p is the
*fork-parent guard* (R9): the FUTURE_WRITE seal + the `Frozen` write denial. `Faulted`
belongs to the fault/stop machinery (the `StopReason::FAULTED` path and `DestroyVm`/
`RestoreSnapshot` recovery), which is a different bead's concern. Adding a half-modeled
`Faulted` here — with no transitions into it and no fault detection wiring — would be
worse than omitting it: it would imply a contract the code does not enforce.

**However**, when `Faulted` is eventually added, two transitions in *this* relation will
need revisiting: `Running → Faulted` and `Paused → Faulted` (fault discovered at a
boundary), and `Faulted → Empty` (Destroy) / `Faulted → Paused` (Restore-into). I
recommend filing a follow-up bead so the omission is tracked rather than rediscovered.
See `04-action-items.md` (Suggestions). This is a tracking note, not a blocker.

### C-B. `ensure_write_path` gates writes but there is no read-path gate; introspection on `Frozen` is unguarded here

API.md §2.6 line 52 states introspection (`ReadGuestMemory`) "slot must be **Paused**".
A `Frozen` parent is therefore *not* a legal introspection target either — yet this
change models only a write gate, with no `ensure_read_path` / `ensure_introspectable`
equivalent.

**Position: not a defect in d2p.** d2p's mandate is the R9 *write* corruption guard;
read-path admission control is a property of the introspection RPC handler (a future
boundary-engine/QMP bead), which will check `state == Paused` directly at the RPC
edge. Reading a `Frozen` parent's RAM is also memory-safe (the seal permits read-only
mappings; the existing mapping is readable), so the absence of a read gate here cannot
cause corruption — it is purely an API-admission concern that lives at a different
layer. The doc-comment on `ensure_write_path` is already scoped to "APIs that can
mutate", so it does not over-claim. I recommend the introspection bead enforce the
`Paused`-only rule at the RPC layer; no change needed in this diff.
