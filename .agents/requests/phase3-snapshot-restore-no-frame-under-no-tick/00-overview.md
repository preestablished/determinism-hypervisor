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

## Scope honesty: what we have and have NOT isolated

We have proven: (a) the workload frames no-tick in VmHarness on boot **and**
restore; (b) the deployed **dh-workerd restore** path does **not** frame. We
have **not** yet tested a **fresh boot through `dh-workerd`** under no-tick, so
we cannot yet distinguish:

- **H1 — restore-specific:** dh-workerd restore fails to re-arm the ring-W
  drain / doorbell servicing that a fresh boot establishes; or
- **H2 — dh-workerd-general:** dh-workerd's no-tick `Run` frame/drain path
  fails on **boot or restore** alike (VmHarness passing only proves *VmHarness*
  drains, not dh-workerd).

The single experiment that decides this — a dh-workerd fresh-boot →
`Run(frame_budget)` control — is the first item in `03-ask.md`. We flag this
openly rather than assert "restore-specific," a narrowing this project has been
burned by before.

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

Both surviving hypotheses (H1, H2) live in dh-worker / dh-vmm. The ask
(`03-ask.md`) starts by disambiguating them.
