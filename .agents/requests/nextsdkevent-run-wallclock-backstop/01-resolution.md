# Resolution: No Wall-Clock Backstop Needed — Closed With Empirical Evidence

Resolved 2026-07-07 (bead `determinism-hypervisor-qq20`). Answer to the
request's two open questions, each settled by a permanent regression test
run on the kvm-intel lab lane, with the architecture argument as
corroboration. **No backstop was implemented — none is needed.**

## Question 1: does an idle HLT (IF=1, no tick) block inside KVM_RUN?

**No.** It exits to userspace and stops the run.

- **Why, architecturally:** this VMM never creates an in-kernel irqchip
  or PIT — `crates/dh-vmm/src/lib.rs:170-189` lists
  `no_in_kernel_irqchip` in the forbidden-capability assertions, and
  `crates/dh-vmm/src/kvm.rs:955-964` (the forbidden-list smoke test)
  documents "We never call KVM_CREATE_IRQCHIP/KVM_CREATE_PIT2". Without an
  in-kernel irqchip, KVM cannot emulate HLT in-kernel ("block until
  interrupt" has no kernel-side path here) — every guest `HLT` produces
  `KVM_EXIT_HLT` back to userspace. `runctl.rs`'s exit handler turns any
  `VcpuExit::Hlt` into `StopReason::GuestHalted` (handler at
  `runctl.rs:632`, unwind at `:734`); it does not distinguish idle from
  terminal HLT.
- **Empirically:** permanent test
  `runctl::event_until_tests::next_sdk_event_idle_hlt_stops_guest_halted_not_wedged`
  (`crates/dh-vmm/src/runctl.rs`), guest `tests/nanokernel/asm/idle_hlt.asm`
  (`sti; hlt` park — the epoll-parked-agent shape), run under
  `Until::NextSdkEvent { hard_cap: 1_000_000_000 }` with no event ever
  fed. Observed 2026-07-07 on the lab lane: prompt return,
  `StopReason::GuestHalted`, icount far below the can't-fire cap; whole
  suite finishes in <1s. The test wraps the run in a 60s watchdog thread
  so a future regression manifests as a test failure, not a hung job.

## Question 2: is there a non-HLT block that burns no instructions and never exits?

**None constructible.** What was tried and observed:

- **MONITOR/MWAIT** (the only non-HLT wait instruction candidate):
  permanent test
  `runctl::event_until_tests::next_sdk_event_mwait_park_cannot_wedge_kvm_run`,
  guest `tests/nanokernel/asm/mwait_park.asm` (MONITOR/MWAIT on a .bss
  line, then a PAUSE spin). Observed 2026-07-07: MWAIT executes as NOP
  (MONITOR is not exposed in guest CPUID), the guest falls into the
  PAUSE spin, retires instructions, and the icount safety net stops the
  run — `stopped: HardCap at icount 300000`, exactly at the cap.
- **PAUSE loops** retire instructions by definition → the icount hard
  cap bounds them. The hard-cap-under-NextSdkEvent machinery itself is
  covered by the pre-existing permanent regression test
  `next_sdk_event_without_events_hits_the_hard_cap_live`
  (`runctl.rs`, asserts `StopReason::HardCap` at exactly icount 300,000
  with a never-bumping feed).
- With no in-kernel irqchip, no PIT, and no kvmclock, the known
  in-kernel blocking sources are absent; there is no remaining mechanism
  by which `KVM_RUN` can sleep on guest-visible state in this VMM.

## Consequences for the bridge

- **The `timeout(1)` stopgap can be retired** (per your
  `03-verification-offer.md`). Every dead-workload shape returns: idle
  or terminal HLT → `GuestHalted`; anything that retires instructions →
  `HardCap` at the icount cap.
- **Semantics note:** an idle-parked guest returns `GuestHalted` — a
  caller polling for READY should treat `GuestHalted` on a
  `NextSdkEvent` run as "workload dead/parked". That is exactly the
  distinguishable signal a wall-clock backstop would have provided, at a
  deterministic icount instead of a nondeterministic deadline.
- One wall-clock deadline does already exist in the run path — the
  RunWithFrameCapture stalled-consumer watchdog
  (`crates/dh-worker/src/service.rs:1493-1568`, `FrameSinkFlow::Stop`
  reason `"watchdog"`, landing at `runctl.rs:741-748`) — but it applies only
  to capture runs (plain `Run` passes no frame sink, `service.rs:3738`)
  and acts only at frame-mark boundaries. It could never unwedge a
  `KVM_RUN` that doesn't exit, and per the above, no such `KVM_RUN`
  exists here.

## Determinism note

Nothing was added to the run path — the probes are tests only, so the
"backstop must not perturb execution" constraint in the request is
satisfied vacuously.
