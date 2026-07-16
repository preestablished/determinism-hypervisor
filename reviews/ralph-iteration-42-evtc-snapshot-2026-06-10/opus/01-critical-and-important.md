# Critical and Important Findings

## Critical

None.

---

## Important

### I-1 — Undocumented non-serialized state: the responder's `FaultPlan` accumulators

**File:** `crates/dh-devices/src/detchannel.rs` — `snapshot()` / `restore()` doc
comments and field handling.

The diff is disciplined about documenting what it deliberately does *not*
serialize — the manifest snapshot (guest RAM, re-read on attach) and the channel's
intern/pending-inject caches (orchestrator-reconstructible per guest-sdk
`channel.rs`). That discipline has one gap: `DetChannelHost` also owns
`responder: InjectResponder<P>` where `P: FaultPlan`, and the production plan type
`TableFaultPlan` (guest-sdk `inject.rs`) carries *accumulating* state:

```rust
// guest-sdk/crates/detguest-host/src/inject.rs
pub struct TableFaultPlan {
    hits: Vec<u32>,                          // per-rule hit counts
    pub decisions: Vec<(u32, FaultDecision)>, // append-only decision log
}
```

`restore()` neither serializes nor resets the responder. For the **replay-mode**
plan (`LogFaultPlan`, used in the tests) this is correct — replay reconstructs its
decisions from the input log, so dropping in-memory accumulators is the intended
behavior and matches the "reconstructible" doctrine. But:

1. The omission is **undocumented**. Every other non-serialized field has an
   explicit "NOT serialized, by design" note with the reconstruction story; the
   responder/`FaultPlan` state has none. A reader cannot tell whether it was
   considered or forgotten.
2. For a **recording-mode** fork (where the synthesizer's `TableFaultPlan` is live),
   a child created via restore inherits whatever responder the *fresh host*
   constructor was given — `restore()` does not touch it. That is plausibly the
   right seam (the orchestrator hands each restored slot its own plan), but the
   coupling is implicit and load-bearing for determinism.

**Recommendation:** Add a sentence to the `snapshot()`/`restore()` docs stating that
the `responder`'s `FaultPlan` state is intentionally not serialized — the
orchestrator supplies the plan at construction (replay: log-backed and
self-reconstructing; recording: the synthesizer's plan), so `restore()` adopts the
fresh host's responder unchanged. This closes the documentation gap without a code
change. Judged **Important** (not Critical) because the current behavior is most
likely correct for the M4 replay path; the risk is a future reader mis-reasoning
about fork-time determinism in recording mode from undocumented intent.
