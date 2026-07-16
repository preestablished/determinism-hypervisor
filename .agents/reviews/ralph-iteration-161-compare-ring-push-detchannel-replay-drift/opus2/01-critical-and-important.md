# Critical And Important Findings

## Critical

No critical production correctness issues found in the current patch.

## Important

I1. Add a negative reseal-equivalence regression test for RING_PUSH payload drift.

References:
- `crates/dh-worker/src/replay_engine.rs:576` defines the generated detchannel predicate; `crates/dh-worker/src/replay_engine.rs:581` newly includes `EVENT_RING_PUSH`.
- `crates/dh-worker/src/replay_engine.rs:772` accepts logs as equivalent when generated-output position normalization makes the comparable records equal.
- `crates/dh-worker/src/replay_engine.rs:749` still includes the record payload in the comparable record, so the current code does reject RING_PUSH payload drift.
- `crates/dh-worker/src/replay_engine.rs:2192` only invokes the classifier after `reseal_equivalent_ignoring_bisection_checkpoints` returns false.
- `crates/dh-worker/src/replay_engine.rs:2614` tests RING_PUSH payload drift by calling `classify_reseal_divergence` directly, bypassing the production equivalence gate.
- `crates/dh-worker/src/replay_engine.rs:2874` covers the positive case for generated detchannel output icount drift.

Why this matters: the bead's key guardrail is that RING_PUSH payload/effect drift must not be accepted or mislabeled as `channel_mutation_drift` before channel-memory effects are applied or compared. The current implementation satisfies that because normalized comparison still compares payload bytes, but the test suite does not lock that down through the same branch production uses. If a later edit accidentally broadens normalization to ignore generated-event payloads, VerifyReplay could accept a changed RING_PUSH payload at `crates/dh-worker/src/replay_engine.rs:2192` and never reach the `skipped_input` classifier asserted by the new test.

Suggested test: add a unit test beside `reseal_classifier_keeps_ring_push_payload_drift_as_skipped_input` that builds `expected = log_with_ring_push(4, 0xAA)` and `got = log_with_ring_push(4, 0xBB)`, then asserts:

```rust
assert!(!reseal_equivalent_ignoring_bisection_checkpoints(&got, &expected_reader).unwrap());
```

That would directly pin the production acceptance path while keeping the current classifier assertion.
