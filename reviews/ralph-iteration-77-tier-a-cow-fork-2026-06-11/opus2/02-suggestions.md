# Suggestions

## S1 — `ForkError::Capture` and `ForkError::Apply` are unreachable in tests

**File:** `crates/dh-worker/src/fork_engine.rs:146–152`,
`crates/dh-worker/tests/fork_engine.rs:591` (`fork_preconditions_fail_loudly`).

`fork_preconditions_fail_loudly` reaches `AgendaNotEmpty`, `ParentNotFrozen`, and
`Kvm` (unsealed). It never reaches `Capture` or `Apply`. The project's testing
research file explicitly says "each documented error enum variant should be
reachable." `Capture` only fires if `build_dhsnap` returns a codec error and `Apply`
only on a non-Kvm `apply_dhsnap` error — both hard to provoke with a healthy bus,
but a shape-mismatch case would hit `Apply` cheaply: fork with a `child_bus` whose
shape differs from the parent's (e.g. two pv-entropy devices, or a missing device),
which makes `apply_dhsnap`'s shape checks fire `RestoreError::Codec` →
`ForkError::Apply`. This also covers the "missing negative: fork with a mismatched
child_bus shape" gap. Worth one added test case.

## S2 — `ForkError::Capture`/`Apply` collapse structured variants into strings

**File:** `crates/dh-worker/src/fork_engine.rs:148–152`, mapping at lines 205–208
and 217–222.

`ForkError::Capture(String)` and `Apply(String)` wrap `EngineError`/`RestoreError`
via `format!("{other:?}")`, discarding the structured variant. A `ConfigMismatch`
becomes an opaque string indistinguishable from a `Codec` error to a programmatic
caller. Two of `RestoreError`'s most caller-relevant variants — `ConfigMismatch`
and `Codec` — are exactly the ones a slot manager might want to branch on (retry vs.
fatal). Consider either `Apply(RestoreError)` (carry the source) or at least adding
a `ConfigMismatch` arm to the match so the most actionable case keeps its identity.
Same reasoning the codebase already applies by special-casing `RestoreError::Kvm`
→ `ForkError::Kvm` (fork_engine.rs:219) — the other high-value variant deserves
the same treatment.

## S3 — `fork_slot`'s 9 positional parameters are a maintainability trap

**File:** `crates/dh-worker/src/fork_engine.rs:176–186` (and the `#[allow(clippy::
too_many_arguments)]`).

Nine positional args, four of them `&`-of-similar types (`&SlotVm`, `&MmioBus`,
`&DetEntropy`, `&mut MmioBus`), invite a silent caller-side transposition — e.g.
swapping `parent_bus` and `child_bus` would compile (both `MmioBus`, differing only
in `&`/`&mut`) and corrupt the child while reading a mutated parent. A `ForkRequest`
struct (mirroring how `BoundaryState` already bundles four scalars) would make the
call site self-documenting and transposition-proof. The `restore_snapshot` signature
has the same smell; if a refactor is done, do both consistently.

## S4 — `forked_child_snapshots_to_the_parents_exact_ref` re-snapshots the parent at `Paused`, not at the frozen fork point

**File:** `crates/dh-worker/tests/fork_engine.rs:664–735`.

The parent is snapshotted while `Paused` (line 684) and the fork happens after a
separate `freeze_ram` (line 697). The ref-identity claim holds because `freeze_ram`
does not change any snapshotted bytes — but the test would be strictly stronger (and
match the production sequence: snapshot a *frozen* parent, or assert the parent's
post-freeze re-snapshot equals its pre-freeze ref) if it proved freezing is a no-op
for the ref. As written it implicitly assumes that; an explicit assertion
(`snapshot(parent_paused) == snapshot(parent_frozen)`) would pin it.

## S5 — Document where `ForkOutcome.child`'s `SlotState` is assigned

**File:** `crates/dh-worker/src/fork_engine.rs:155–168` (`ForkOutcome`),
doc at lines 170–174.

`ForkOutcome` carries no `SlotState`; the doc says the child is "Paused at the
parent's boundary." But nothing in this module sets, returns, or attests that state
— it is silently the slot manager's job (consistent with the parent-Frozen
bookkeeping note). For a reader wiring this up, an explicit one-liner on `child`
("the caller MUST register this as `SlotState::Paused`; this engine does not track
slot state") would remove the ambiguity, matching how `restore_engine` documents the
"caller must discard a scrap slot" contract.
