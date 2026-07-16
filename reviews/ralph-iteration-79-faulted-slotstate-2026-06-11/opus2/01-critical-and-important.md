# Critical & Important

## Critical

None.

---

## Important

### I-1. Proto↔domain `SlotState` mapping is now a latent foot-gun: the two enums differ in BOTH offset AND order — a future `as i32` cast would silently corrupt state reporting

**Where:** `crates/dh-vmm/src/lib.rs:41-54` (domain enum) vs
`proto/hypervisor.proto:414-415` and `crates/dh-proto/src/lib.rs:163-168`
(wire enum).

**The mismatch.** The two enums are not merely offset-by-one; they are in a
**different order**:

| domain (`dh_vmm::SlotState`) | discriminant | proto (`dh_proto::v1::SlotState`) | value |
|------------------------------|:---:|------------------------------------|:---:|
| —                            |  —  | `SLOT_UNSPECIFIED`                 |  0  |
| `Empty`                      |  0  | `EMPTY`                            |  1  |
| `Running`                    |  1  | `PAUSED_S`                         |  2  |
| `Paused`                     |  2  | `RUNNING`                          |  3  |
| `Frozen`                     |  3  | `FROZEN`                           |  4  |
| `Faulted`                    |  4  | `FAULTED_S`                        |  5  |

Note `Running` and `Paused` are **swapped** between the two
representations (domain: Running=1/Paused=2; proto: PAUSED_S=2/RUNNING=3). So
the bug is not catchable by a naive "off-by-one" sanity check or an
`x as i32 + 1` shim — those would map domain `Running`(1) → proto `PAUSED_S`(2)
and domain `Paused`(2) → proto `RUNNING`(3), i.e. report a paused slot as
running and vice-versa. For a determinism hypervisor, mislabelling run state on
the wire is exactly the class of silent corruption this codebase is built to
avoid.

**Why this is the right time to flag it.** No conversion exists today (verified:
no `From<SlotState>`, no `impl TryFrom`, no `as i32` against the domain enum
anywhere in `crates/`). Bead 324 is the iteration that *introduces* the proto
`FAULTED_S` correspondence in the domain enum's own doc comment
(`lib.rs:48` references "proto FAULTED_S / StopReason::FAULTED"), so this is the
moment the two enums become semantically paired in a reader's mind — and the
moment someone is most likely to reach for `domain_state as i32` when wiring the
GetSlotState RPC.

**Recommended action (not a blocker for this diff):** File a bead to pin the
mapping as a hand-written exhaustive `match` (one arm per variant, both
directions) with a round-trip test, and add a one-line note at the domain enum
or in the bead that **`as i32` casting between these enums is forbidden**. The
existing proto-numbering test in `dh-proto/src/lib.rs:163-168` already pins the
wire side; the missing half is the domain→wire match and its test. This is
cheap to do correctly now and expensive to discover later (it would manifest as
a wrong-state report under fault, the least-tested path).

**Severity rationale:** marked Important rather than Critical because the
defect does not exist yet — there is no incorrect code in this diff. It is a
guardrail against a near-future change. If a reviewer prefers, it can be
downgraded to a Suggestion; I rank it Important because the order-swap makes the
eventual bug invisible to casual inspection.
