# Request: dh-workerd produces no post-Ready frame under no-tick

- **From:** rom-operator-bridge (Phase 3 deployed-bridge first-frame cutover)
- **To:** determinism-hypervisor (dh-worker / dh-vmm — no-tick frame path)
- **Date:** 2026-07-05
- **Priority:** P0 — sole remaining blocker for Phase 3 "workload-in-the-box"
  first real frame in the browser.
- **Tracking:** rom-operator-bridge-9xo (sender-side dependency bead).

## One-paragraph summary

The deployed bridge cutover to the real workload succeeded: the deployed
`dh-workerd` restores the regenerated Ready snapshot and executes it. But the
restored guest never produces a frame under the no-tick deterministic worker —
`Run{until: frame_budget=1}` burns the full default hard cap (`10e9`
instructions) with **zero frames and zero drained GuestEvents**. In contrast,
guest-sdk's in-process `VmHarness`, running the byte-identical workload with
the same timerless cmdline, frames within milliseconds of Ready — and
(per guest-sdk's own resolution) does so on **both a fresh boot and after
`VmHarness::from_snapshot()`**. So *within VmHarness, neither boot nor restore
is broken.* The break lives on the **VmHarness → dh-workerd** axis.

## Isolation: the deciding control was RUN — it is H2, not restore-specific

We considered two hypotheses and ran the experiment that decides them:

- **H1 — restore-specific:** dh-workerd restore fails to re-arm the ring-W
  drain / doorbell servicing that a fresh boot establishes; or
- **H2 — dh-workerd-general:** dh-workerd's no-tick `Run` frame/drain path
  fails on **boot or restore** alike (VmHarness passing only proves *VmHarness*
  drains, not dh-workerd).

**Result: H2.** We ran determinism-hypervisor's own `#[ignore]`d worker test
`linux_m5_frame_budget_records_post_ready_frame_marks`
(`crates/dh-worker/tests/m5_frame_scheduling.rs:50`) against the real
reference-workload artifacts. Its **first** `Run{frame_budget}` — on a
**freshly-booted** VM (`CreateVm` → run-to-`Ready`, **no snapshot**) — fails:
`"first Linux run stopped with reason 4, expected BudgetReached"` (reason 4 =
`HARD_CAP`; it never reaches the restore arm). So the workload boots to `Ready`
through dh-worker fine, but **post-`Ready` framing hits the hard cap on a fresh
boot** — the identical no-frame symptom as the deployed restore, with no
snapshot involved. Meanwhile the same workload with the same no-tick flags
frames in VmHarness. The failing variable is **VmHarness → dh-worker/dh-vmm**,
present on boot *and* restore. Restore is **not** special; the re-attach path
(H1) is at most a secondary contributor. Full command + output in
`01-evidence.md`.

## Why this is determinism-hypervisor's and not guest-sdk / reference-workload

- guest-sdk's frame primitive is **not** broken, and guest-sdk did **not**
  overclaim: their M4 fixture frames no-tick, and their real-artifact
  `refwork_ready_hold` no-timer arm — which *they* could not run
  (`REFWORK_READY_INITRAMFS` unset) — **passes** when we point it at the real
  deployed initramfs. Their resolution even **predicted this class of bug**:
  "A downstream worker that still misses post-Ready frames should first verify
  its ring-W drain and NextSdkEvent(FrameMark) stop path at the pv-pad
  FRAME_COUNTER exit."
- reference-workload's emulator is the same binary in the passing (VmHarness)
  and failing (dh-workerd) cases.
- rom-operator-bridge is exonerated: our direct `grpcurl`
  `RestoreSnapshot → Run(frame_budget=1)` reproduces the failure with the
  bridge entirely out of the loop (`01-evidence.md`).

The confirmed defect (H2) lives in dh-worker / dh-vmm — the no-tick `Run`
frame/drain path, on the boot path itself. The ask (`03-ask.md`) is scoped to
that, with your own `linux_m5` test as the ready-made red repro.
