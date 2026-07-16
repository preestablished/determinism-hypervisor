## Action Items

### Critical
- None

### Important
- None

### Suggestions
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:637] Either assert `expected.txt`'s `records_applied` value during re-verification or remove it from the manifest.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:788] Consider making an explicitly selected regeneration test fail when `DH_WORKER_REGEN_RR_CORPUS` is missing, so omitted env setup cannot look like a successful re-baseline.
- [ ] [crates/dh-worker/tests/m5_record_replay.rs:263] Consider enforcing the exact expected-manifest key set so stale extra fields cannot survive future hand edits.
