# Critical And Important Findings

No critical findings.

## Important: the gate does not verify workload-observed decisions

References:

- `crates/dh-worker/tests/gs7_inject_replay.rs:209`
- `crates/dh-worker/tests/gs7_inject_replay.rs:219`
- `crates/dh-worker/tests/gs7_inject_replay.rs:229`
- `crates/dh-worker/tests/gs7_inject_replay.rs:248`
- `.agents/docs/guest-sdk/IMPLEMENTATION-PLAN.md:147`
- `.agents/docs/guest-sdk/IMPLEMENTATION-PLAN.md:151`

The test verifies that StreamGuestEvents contains `InjectQuery` records, that the DHILOG contains the same number of `PORT_INJECT` `PIO_ANSWER` records, that at least two raw answer values are nonzero/distinct, and that `VerifyReplay` reaches the same end hash. It never parses a fixture-provided workload observation of the returned decisions.

That leaves a false-pass path: the workload can call `inject_point`, the host can record nontrivial answers, and replay can reproduce the same final state, while the workload ignores, misdecodes, reorders, or fails to apply those decisions. A final state hash match does not prove decision observation if the fixture's state is not made sensitive to those values. The guest-sdk acceptance text explicitly calls for "all inject decisions observed by the workload (echoed via LogLine digest)."

The GS-7 gate should require the sibling fixture to emit an observable decision sequence, for example a `LogLine` digest or published region, and compare it exactly against the decoded DHILOG answer sequence. Ideally the comparison should include occurrence order and enough query identity (`iseq`, `name_id` or interned name) to prove the workload observed the same decisions at the expected inject points.

## Important: post-READY query evidence is not isolated from the boot/READY event backlog

References:

- `crates/dh-worker/tests/common/mod.rs:481`
- `crates/dh-worker/tests/common/mod.rs:507`
- `crates/dh-worker/tests/gs7_inject_replay.rs:181`
- `crates/dh-worker/tests/gs7_inject_replay.rs:209`
- `crates/dh-worker/tests/gs7_inject_replay.rs:219`
- `crates/dh-worker/src/service.rs:7349`
- `crates/dh-worker/src/service.rs:4197`

`m9_linux_ready_snapshot` runs until READY and takes the READY snapshot, but it does not drain StreamGuestEvents. The worker intentionally keeps the `RunResponse.sdk_event` in the StreamGuestEvents backlog. The new GS-7 test only streams events after the post-READY frame run, then compares all streamed `InjectQuery` events with the `PIO_ANSWER` records from `post_snapshot.input_log_id`.

That log id is the segment sealed after the READY snapshot. If the future guest-sdk/reference-workload fixture emits any `InjectQuery` before READY, those query events can still be present in the later stream, while their matching `PIO_ANSWER` records belong to the earlier READY-segment DHILOG. The count comparison can then fail a valid post-READY fixture, or, if counts accidentally align, reason about the wrong segment.

The test should either drain and discard the StreamGuestEvents backlog immediately after the READY snapshot and before the GS-7 run, or filter/assert on `event.icount` so only events after `ready.ready_snapshot.icount` participate in the query/answer comparison. If pre-READY inject traffic is forbidden for this gate, assert that explicitly instead of mixing it into the post-READY evidence.

## Important: nontrivial decision counting uses raw nonzero values, not decoded `FaultDecision` semantics

References:

- `crates/dh-worker/tests/gs7_inject_replay.rs:229`
- `crates/dh-worker/tests/gs7_inject_replay.rs:240`
- `../guest-sdk/crates/detguest-wire/src/ports.rs:121`
- `../guest-sdk/crates/detguest-wire/src/ports.rs:124`

The nontrivial filter is `answer.value != FaultDecision::Proceed.pack()`. That treats every raw nonzero PIO answer as non-Proceed. The wire decoder is more specific: `FaultDecision::unpack` treats kind `0` as `Proceed`, regardless of the upper arg bits. A malformed or noncanonical raw value such as `0x00000100` would be counted as a nontrivial distinct decision by this gate, even though the guest SDK decodes it as `Proceed`.

For an acceptance gate that claims nontrivial `FaultDecision` coverage, count decoded decisions:

- Decode with `FaultDecision::unpack(answer.value)`.
- Treat only decoded `Platform` or `Workload` decisions as nontrivial.
- Prefer rejecting noncanonical values where `FaultDecision::unpack(value).pack() != value`, so malformed PIO payloads cannot satisfy the gate.
