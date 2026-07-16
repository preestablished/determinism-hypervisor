# Overview

Scope reviewed: Ralph iteration 161 checkpoint `55fe7fddb260c4053e2d340663e97c005164bbc7` on branch `ralph/iteration-161-compare-ring-push-detchannel-replay-drift`, compared against `main` at `5e57911643eef41d9e85de0c3052bf1ecb8a149e`.

The implementation changes only `crates/dh-worker/src/replay_engine.rs`. It adds detchannel `EVENT_RING_PUSH` to generated-output position normalization, keeps RING_PUSH out of the `channel_mutation_drift` classifier, and adds unit coverage for RING_PUSH payload drift being classified as `skipped_input`.

Review result: I did not find a production correctness blocker in the current patch. The main issue is a missing negative comparator test: the new classifier test calls the classifier directly, but production only reaches that classifier after the reseal equivalence check rejects the replayed log.

Verification run:

```text
cargo test -p dh-worker --lib replay_engine::tests::reseal -- --nocapture
cargo test -p dh-worker --lib replay_engine::tests::replay_detchannel -- --nocapture
git diff --check main...HEAD
```

All commands passed.
