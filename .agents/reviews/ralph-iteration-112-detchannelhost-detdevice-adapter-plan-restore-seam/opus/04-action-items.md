## Action Items

### Critical
- None

### Important
- None

### Suggestions
- [ ] [crates/dh-devices/src/detchannel.rs:318] Reject EVTC flag bytes other than `0` or `1` so malformed detchannel sections cannot restore as canonicalized state.
- [ ] [crates/dh-worker/tests/restore_engine.rs:245] Add a negative restore-engine EVTC test that proves an attached EVTC section fails when restored RAM lacks a valid channel header.
- [ ] [crates/dh-worker/src/replay_engine.rs:287] Track the remaining detchannel `DEV_EVENT` replay/runtime composition if this adapter bead is expected to close the broader ol1 thread.
