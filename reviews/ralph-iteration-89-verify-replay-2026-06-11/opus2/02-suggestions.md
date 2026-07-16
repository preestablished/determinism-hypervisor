# Suggestions

### S-1 — HeaderMismatch as `Err` is the RIGHT call for cw2; document the contract explicitly

The prompt asks whether a log whose `base_snapshot_id` doesn't match the given
ref (or `machine_config_hash`, or clock ratio) should be a `Divergence`
verdict or an infrastructure `Err`. Today these are `ReplayError::HeaderMismatch`
→ propagated as `Err(other)` by the wrapper (verify_replay.rs:85).

**I agree with the current classification.** A header mismatch is a *usage*
error — the caller paired the wrong (snapshot, log) or the wrong config — not
a statement that the recorded machine is non-deterministic. cw2 will
VerifyReplay 1000 (snapshot, spliced-log) pairs; a mispairing in the harness's
own bookkeeping must **fail loudly as an error**, not be silently logged as
"child 437 diverged" and counted toward the zero-Divergence exit gate. A
`Divergence` verdict is a real, alarming product-property failure; conflating a
test-harness wiring bug with it would poison the exit-gate signal.

**Recommendation:** Add one line to the wrapper's doc-comment making this
explicit: "HeaderMismatch (wrong snapshot/config pairing) is an infrastructure
`Err`, NOT a `Divergence` verdict — a mispaired input is a usage error, and
cw2's zero-Divergence exit gate must not absorb wiring bugs." Right now the
distinction lives only in the engine's error enum; the *reporting* layer should
state it because that's the layer cw2 reads.

### S-2 — What cw2 (the 1000x harness) needs from this API — and what is missing today

From `bd show determinism-hypervisor-cw2`: boot → root snapshot → 1000 tier-A
forks across slots, each a distinct seeded pad-burst, TakeSnapshot each,
**VerifyReplay every (snapshot, spliced log) → 1000/1000 VerifyDone, zero
Divergence.** Mapping that onto this API surfaces three gaps:

1. **Store / counter reuse across children.** `verify_replay` takes `&mut
   SlotVm`, `&InstRetired counter`, `&SnapstoreClient store` per call — good,
   they're borrowed, so a slot pool + one store can be reused across 1000
   calls. But the `DeviceRail` is moved by value per call (correct — it's
   sealed). The harness will need a cheap per-child rail factory; nothing in
   this diff blocks that, but there's no helper for it yet. **Note for cw2:**
   the test's inline `DeviceRail::new(...)` block (replay_engine.rs test
   :377-388, duplicated for slot2) is the pattern that should become a shared
   fixture, not be copy-pasted 1000 ways.

2. **Per-child verdict aggregation.** `VerifyReport` is per-run. cw2 needs to
   fold 1000 reports into "1000/1000 verified, 0 diverged" and, on any
   failure, name *which* child + *which* epoch. The current `verified()` /
   `divergence()` accessors support this, but there is **no batch type**
   (`Vec<(ChildId, VerifyReport)>` summarizer). cw2 will have to build it.
   Recommend a small `VerifyBatch` in dh-verify (pure types) that holds
   `Vec<VerifyReport>` and exposes `all_verified()`, `divergences()` →
   `impl Iterator<Item=(usize, &VerifyProgress)>`. Keep it in dh-verify so the
   dependency direction stays clean.

3. **Divergence field fidelity (see I-2) is a cw2 blocker-in-waiting.** When
   one of 1000 children fails on `end_state_hash` or `resealed log bytes`, the
   operator gets `first_bad_epoch` — which for those shapes is misleading or
   nonsense. Resolve I-2 before cw2 relies on these fields for triage.

No batching/streaming primitive is *required* for the M5 demo, but cw2 will
have to add (2) and lean on the resolution of (3). Worth a follow-up bead now.

### S-3 — `epoch_len.max(1)` guards div-by-zero but masks an invariant

`first_bad_epoch: at_icount / machine_config.epoch_len.max(1)` (verify_replay.rs:77)
guards against `epoch_len == 0`. But `MachineConfig::validate` (config.rs:152)
*rejects* `epoch_len == 0`, and `replay_segment` already computed
`config_hash()` (which would have surfaced a malformed config) before any
Divergence is possible. So `epoch_len` is provably ≥ 1 here. The `.max(1)` is
defensive but hides that invariant. Minor: either drop it (config is
validated) or comment that it's belt-and-suspenders for an unvalidated config.

### S-4 — Test over-asserts on the magic count `10`; derive it

`assert_eq!(report.epochs_ok(), 10)` (test) hard-codes `3 * QUANTUM /
epoch_len = 3*100_000/30_000 = 10`. This is correct but brittle: if a future
tweak to `QUANTUM` or `epoch_len` in the fixture changes the grid, the test
breaks with a bare `10 != N` and no hint why. Per the research file's
"over-asserting on incidental details" pitfall, derive it:
`let expected_epochs = (3 * QUANTUM) / cfg.epoch_len;` and assert against that.
The intent ("one EpochOk per epoch on the absolute grid") becomes legible.

### S-5 — Use `assert_matches!`-style for the Divergence destructure

The test's `match report2.divergence().unwrap() { VerifyProgress::Divergence
{..} => {...}, other => panic!(...) }` is the exact `if let { panic!() }`
chain the research file (rust-integration-testing.md) flags. With
`assert_matches` (or the inline `let VerifyProgress::Divergence { .. } = x
else { panic!() }` let-else form, no dep needed) the assertion is tighter.
Minor ergonomics; the current form is correct.
