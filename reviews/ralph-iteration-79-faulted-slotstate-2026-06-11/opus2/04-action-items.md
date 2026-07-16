# Action Items

Each item is self-contained; none block merging this diff.

### Critical

None.

### Important

- [ ] **Pin the `dh_vmm::SlotState` ↔ `dh_proto::v1::SlotState` mapping before
  any RPC wires it; forbid `as i32` casting.** The two enums differ in offset
  AND order: domain `Empty=0, Running=1, Paused=2, Frozen=3, Faulted=4`; proto
  `EMPTY=1, PAUSED_S=2, RUNNING=3, FROZEN=4, FAULTED_S=5` (note Running/Paused
  are swapped). No conversion exists today, so a future `state as i32` shim
  would silently report Running-as-Paused and vice-versa. File a bead to add a
  hand-written exhaustive `match` in both directions with a round-trip test, and
  add a one-line "no `as i32` between these enums" note. Source of the pairing
  pressure: `crates/dh-vmm/src/lib.rs:48` doc now references proto `FAULTED_S`.
  (Detail: `01-critical-and-important.md` I-1.)

### Suggestions

- [ ] **Re-scope the `Frozen→Faulted` doc from a closed proof to a scoping
  decision, and bead the host-side fault question.** The rationale at
  `lib.rs:100-103` ("a frozen parent executes nothing … nothing can fault it")
  holds only for *guest-contract* faults. A host-side failure on a frozen parent
  (seal read-back failure, bad KVM fd, unreadable CoW baseline) has no good sink
  today — `Frozen→Empty` would destroy a parent that still backs live children.
  Reword the doc to say the omission covers guest faults and is a cheap one-line
  addition if a host-fault path needs it (the relation is pure in-memory, no
  migration). (Detail: `02-suggestions.md` S-1.)

- [ ] **Add one structural property test that is not a transcription of the
  edge list.** Both the 5×5 matrix and the terminality test re-state the
  `allowed` tuples, so they only detect drift between two copies of the
  relation, not a wrong relation. Add e.g.
  `for s in ALL { assert_eq!(s.can_transition(Faulted), matches!(s, Running |
  Paused)) }` (predecessor property) and/or an "every non-Empty state reaches
  Empty" reachability check. (Detail: `02-suggestions.md` S-2.)

- [ ] **Record that `Faulted` was appended to preserve discriminants.** Add a
  bead/comment note so a future contributor does not reorder `SlotState` into
  alphabetical/lifecycle order and silently renumber it (matters until the
  by-match proto mapping in I-1 lands). (Detail: `02-suggestions.md` S-3.)

- [ ] **(Optional) Evaluate collapsing the three `*WriteDenied` variants into a
  single `WriteDenied { state, api }`.** Trade-off only — the explicit variants
  are self-documenting at the R9 guard; flagging so the choice is deliberate. No
  change required. (Detail: `02-suggestions.md` S-4.)
