# Step 3 — Wall-Clock Backstop: Confirm First, Implement Only If Real

Settles `requests/nextsdkevent-run-wallclock-backstop/` (no resolution file
yet). The filing is explicitly confirm-first: "if HLT handling already covers
it, close this with a note." Do the empirical step 0 before writing any
backstop code.

## The Analytic Prior (state it, then test it)

From `01-current-state.md`: this VMM **never creates an in-kernel irqchip or
PIT** (`dh-vmm/src/lib.rs:170-189` forbidden list; `kvm.rs:960` smoke test).
Without an in-kernel irqchip, KVM cannot handle HLT in-kernel — a guest `HLT`
always produces `KVM_EXIT_HLT` back to userspace, and `runctl.rs:632` turns
*any* `VcpuExit::Hlt` (idle or terminal — it does not distinguish) into
`StopReason::GuestHalted`. So suspect case (a) — idle HLT wedging inside
`KVM_RUN` — should be impossible in this VMM. Suspect case (b) — a non-HLT
block that retires nothing and never exits — has no known mechanism here
(no in-kernel blocking sources; MWAIT/PAUSE either exit or retire), but is
the one the repro must actually probe.

The resolution must rest on the repro evidence, with the architecture argument
as corroboration — not the other way around.

## Step 0 — The Repro Harness

Build two targeted probes under `Run{until: NextSdkEvent}`. Likely home: a
new `#[test]` in `crates/dh-vmm/src/runctl.rs`'s test module or
`crates/dh-worker/tests/` following the existing nanokernel/guest-blob
patterns (the runctl tests at `runctl.rs:1100+` already run tiny guest code
blobs under various `Until` modes — reuse that scaffolding).

**Probe A — idle HLT under no-tick:** guest executes `sti; hlt` (IF=1, no
timer scheduled, no pending SDK event — the epoll_wait-parked-agent shape).
Run with `Until::NextSdkEvent { hard_cap: <large> }` and no matching event
ever fed. Expected: `KVM_RUN` returns promptly with `VcpuExit::Hlt` →
`StopReason::GuestHalted`. Assert the run returns within the test harness's
normal time (wrap in a generous watchdog thread/timeout so a *failure*
manifests as a test failure, not a hung CI job).

**Probe B — non-HLT zero-retirement attempt:** try to construct a guest state
that blocks in `KVM_RUN` without retiring instructions and without HLT.
Candidates to try and document: `mwait` (with/without `monitor`), a
`pause`-loop (expected: retires → icount cap trips → `HardCap` stop). The
hard-cap-under-NextSdkEvent machinery is **already proven** by a permanent
regression test at `runctl.rs:2340-2362` (runs
`Until::NextSdkEvent { hard_cap: 300_000 }` with a never-bumping feed,
asserts `StopReason::HardCap` at exactly icount 300,000) — cite it in the
resolution rather than rebuilding it. If no zero-retirement construction
blocks, say so and list what was tried — a documented failed attempt is the
deliverable.

Run both probes on the lab lane (they are KVM tests). Keep them in the tree
as permanent regression tests, not throwaway scripts — name/comment them as
the resolution's cited harness.

## Decision Gate

- **No hang reproduced (expected):** write the resolution
  (`requests/nextsdkevent-run-wallclock-backstop/01-resolution.md`) with:
  the two probe tests cited by name, the no-irqchip architecture argument
  with file cites, the observed stop reasons/latencies, and the two answers
  the filing asked for: (1) idle HLT reaches `VcpuExit::Hlt` → GuestHalted,
  it does not block in-kernel; (2) no non-HLT zero-retirement hang was
  constructible, plus the hard-cap stop as the bound for anything that
  retires. Explicitly note the bridge may retire its `timeout(1)` stopgap
  (their `03-verification-offer.md` says they will). **Implement nothing.**
  One semantics nuance worth a sentence in the resolution: an idle-parked
  guest returns `GuestHalted` — callers polling for READY should treat
  `GuestHalted` on a NextSdkEvent run as "workload dead/parked", which is
  the distinguishable signal the backstop would have provided anyway.
  Also cite the one wall-clock deadline that already exists in the run path —
  the RunWithFrameCapture stalled-consumer watchdog
  (`crates/dh-worker/src/service.rs:1493-1568`, `FrameSinkFlow::Stop` reason
  `"watchdog"`, landing at `runctl.rs:742`) — and why it does not cover this
  case: it applies only to capture runs (plain Run passes no frame sink,
  `service.rs:3738`) and acts only at frame-mark boundaries, so it could
  never unwedge a `KVM_RUN` that doesn't exit.
- **Hang reproduced (unexpected):** implement the host-side deadline per the
  spec below, and keep the repro as the regression test.

## Implementation Spec (ONLY if a hang reproduces)

Requirements from the request (all mandatory):

1. **Host-side only.** A deadline may abort a Run from the host; it must
   never inject a guest-visible event at a nondeterministic icount.
   Mechanism sketch: a watchdog thread + `pthread_kill`/kvm immediate-exit
   (the same kick mechanism the icount cap PMU path uses) forcing `KVM_RUN`
   to return, then unwind with a new distinct stop/error — do NOT reuse
   `GuestHalted`.
2. **Surface:** per-Run parameter or worker config; document the default and
   the override path for bridge/orchestrator in the proto/config docs.
3. **Distinguishable gRPC status** reported by the worker itself (a new
   status/stop-reason the bridge can render instead of an infinite spinner).
4. **Slot recoverable:** after a backstop abort, `DestroyVm` and
   `RestoreSnapshot` must work without a worker restart — regression test.
5. **Never replayable:** a backstop-aborted run's input log is truncated at a
   nondeterministic point — it must not be committed/sealed as replayable
   evidence. Enforce and test.
6. **Determinism regression test:** record+replay with backstop
   *enabled-but-not-fired* is bit-identical to backstop-disabled (the
   deadline's mere existence must not perturb execution — e.g. no extra
   exits, no timing-dependent agenda changes).

Phase 5 context (why the spec is strict): the orchestrator will run
unattended thousands of Runs; the backstop is the difference between a
retryable error and a wedged slot in a 4-hour soak.

## Acceptance for This Step

- Both probes exist in the tree and ran on the lab lane; outcome documented.
- Resolution file written in `requests/nextsdkevent-run-wallclock-backstop/`
  (either close-with-evidence or backstop-landed), per `05-`.
- If implemented: all six spec points have tests; proto/docs updated; bead
  filed and closed.
