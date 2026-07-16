# Critical & Important Findings

**None.**

No Critical or Important issues were found in this branch. All five
verification axes (completeness, format-freeze discipline, ledger format,
error-variant hygiene, test quality) checked out, and the workspace builds
with the golden / reader_validation / dhilog-lib test suites passing.

The change is correct on its core claim: a zero-length `NET_RX` can never
legitimately be recorded (the device rejects empty delivery at
`net.rs:158` and faults a zero-length TX at `net.rs:105`), so forbidding it at
the codec aligns the writer and reader with a previously-implicit device
invariant. The writer's new guard is unreachable in practice — `net_rx` is
only called from `recording.rs:203`, after `apply_net_rx` (line 201) has
already rejected `len == 0` — making it pure defense-in-depth, which is the
right posture for a codec invariant.
