Reviewer: Carver
Verdict: REQUEST_CHANGES

Scope reviewed:
- `crates/dh-worker/src/snapshot_engine.rs`
- `crates/dh-worker/tests/snapshot_engine.rs`
- `crates/dh-worker/src/service.rs`

Summary:
The checkpoint primitive shape was sound, but it incorrectly inherited
public TakeSnapshot's `agenda_empty` precondition and lacked a paired
capture-vs-no-capture execution equivalence test.
