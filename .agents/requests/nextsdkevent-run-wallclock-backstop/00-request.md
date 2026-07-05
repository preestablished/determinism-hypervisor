# Request: Confirm/Add A Wall-Clock Backstop For NextSdkEvent Runs

Filed 2026-07-05 by rom-operator-bridge, from guest-sdk's
`phase3-boot-scheduling-deadlock` resolution (action item #3). Low
priority — not blocking; a robustness question.

## Context

guest-sdk fixed the Phase 3 boot deadlock by making the agent's pre-Ready
waits **park in `epoll_wait`** instead of spinning. Consequence they
flagged: with the agent parked and the guest getting **no timer tick**, a
genuinely dead workload burns *no* guest instructions — so a
`Run{until: NextSdkEvent(Ready), hard_icount_cap}` cannot rely on the
icount HARD_CAP to bound that failure mode. They ask the worker to own a
wall-clock backstop.

## What We Found (please confirm)

The run loop already treats terminal HLT as a stop
(`crates/dh-vmm/src/runctl.rs:544` — `VcpuExit::Hlt → halted`,
GUEST_HALTED). A fully-blocked guest should idle-HLT and hit that path,
so the "hang forever" case may already be covered. The open questions:

1. Does an **idle HLT** (IF=1, "waiting for an event that never comes"
   under no tick) reach the same `VcpuExit::Hlt` stop, or does KVM block
   in `KVM_RUN` without returning (true hang)?
2. Is there a non-HLT block that burns no instructions and never exits
   (so neither the icount cap nor the HLT path fires)?

If either can hang a `NextSdkEvent` run, add a per-Run **wall-clock
deadline** (host-side, like the guest-sdk harness's `run_until` wall
deadline) as the backstop; if HLT handling already covers it, close this
with a note.

## Evidence It Isn't Urgent

The Phase 3 step-2 handoff (the run that motivated this) **reached READY
and snapshotted** — the happy path is unaffected. We wrapped that run in a
host `timeout(1)` as a stopgap and it completed well within it. So this is
robustness debt for the dead-workload failure mode, not a live blocker.
