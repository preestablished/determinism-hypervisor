# Review 1 — Technical Correctness (subagent, 2026-07-07)

Lens: verify every factual/code claim in plan files 00–05 against the tree.

## Findings

1. **NIT** — 01 header said line numbers were from `bdd476b`; they actually
   match HEAD `4497f60` (`runctl.rs` drifted +4 lines between the revs).
2. **MINOR** — 01 §Staging cited the old-fixture rejection at
   `common/mod.rs:609` (that's the `m9_linux_ready_snapshot` declaration);
   the actual validation is `assert_m9_real_emulator_initramfs`
   (executable `usr/bin/refwork-harness`, `boot.toml` autostart check).
3. **MINOR** — 03 §1 said "the tests print/assert `(icount, frame)` pairs";
   true for `m5_frame_scheduling` (`frame_marks()` :747, eprintln :226-231,
   :341-343) but `m5_net_loopback` prints only a single-line
   `run_icount`/`frame_counter` summary (~:171-175).
4. **MINOR** — 04 Probe B proposed verifying hard-cap-under-NextSdkEvent;
   this already exists as a permanent regression test at
   `runctl.rs:2340-2362` (`hard_cap: 300_000`, never-bumping feed, asserts
   `StopReason::HardCap` at icount 300,000) — cite, don't rebuild. Probe
   scaffolding claim otherwise accurate (test module :1043, blob tests via
   `rig()`, NextSdkEvent uses at :1308/:2297/:2355, HLT→GuestHalted asserts
   :1418/:1487).
5. **MINOR** — 01 cap table omitted `FRAME_HARD_CAP`'s other call sites:
   NOP-game diagnostic (:276) and non-ignored fixture helper `run_frames`
   (:731; callers :973/:1056, can't-fire cap asserting `BudgetReached`) —
   confirms raising the constant is fixture-safe.
6. **MINOR** — 04 didn't mention the one existing wall-clock deadline: the
   RunWithFrameCapture stalled-consumer watchdog
   (`service.rs:1493-1568`, landing `runctl.rs:742`) — non-covering (capture
   runs only, frame-boundary only) but worth citing as precedent in the
   resolution.

## Verified correct (spot checks)

All cap constants/values/usages; detchannel test genuinely synthetic
(`nanokernel::detchannel_frames_elf()` :370, `CreateVm`, never
`m9_linux_ready_snapshot`); `assert_worker_frame_budget` (:799); test names
and cargo filters; all dhilog/replay_engine/detchannel cites; no-irqchip
claims (`lib.rs:170/179/189`, `kvm.rs:960` — smoke test empirically asserts
HLT reaches `VcpuExit::Hlt` with no in-kernel irqchip); `runctl.rs:632`
treats any `VcpuExit::Hlt` as GuestHalted across all `Until` modes; proto
`NextSdkEvent` exists and is mapped; take-two frame table quoted verbatim;
all three request dirs and resolution numbering; guest-sdk sibling checkout.

## Verdict

Factually solid; the central no-irqchip → HLT-exits-to-userspace →
GuestHalted prior is correct in this codebase. Six minor findings, nothing
blocking.
