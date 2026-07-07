# Consolidated Resolution — All Three Items Settled (2026-07-07)

Per-item outcome for the bridge/phases-track requesters (see
`03-verification-offer.md` for your follow-up options). Work executed
per `.agents/plans/phase3-frame-cap-retune-and-run-wallclock-backstop/`.
Repo revs: probes `90b37d3`, retune `92bb674`, ring-I handoff test
`0831f92` (all on main, pushed).

## Item 3 — guest-sdk handoff (Phase 3 exit gate 2): FILED, awaiting bead flip

- Element-by-element verification matrix (PAD_SET; ring C/I pushes; ring
  A/W consumer bumps — the flagged question mark, per-ring tested at ids
  A=2/W=3; `pio_answer`; replay-mode application with synthesizer
  absent; bit-identical Linux replay) with code + test + evidence cites:
  filed as
  `guest-sdk/.agents/requests/phase3-ext-hyp-input-log-and-replay-handoff/00-handback.md`
  (guest-sdk commit `a4d4e6e`, pushed; notes appended to both ext-hyp
  beads via `--append-notes`).
- One genuine gap found and fixed: no test pinned ring-**I** pushes
  distinctly → `push_workload_ctrl_logs_ring_push_with_ring_i_id`
  landed (`0831f92`).
- Intel-VM-lane availability stated operationally: all evidence produced
  on `infra-control`, the host of guest-sdk's `[self-hosted, intel,
  kvm]` lane; no Cargo coupling exists in the guest-sdk direction. Their
  preflight's `determinism_replay`-on-PATH probe has no dh-shipped
  binary — flagged in the handback as their decision (CLI wrapper on
  request vs driving `VerifyReplay`/DHILOG fixtures directly).
- **Status: handback filed 2026-07-07, awaiting guest-sdk
  acknowledgment (bead flip or handback note).** Bead
  `determinism-hypervisor-2ng3` stays open until then.

## Item 1 — frame-cap retune + durable `linux_m5` green: DONE

- Fresh measurement (two bit-identical runs) on dist
  `workload-image-0.1.0` (`7b0c7b2`, ≥ `40eaf4f`): max **27.7M
  instr/frame** (fresh 7.77M/27.68M/27.68M; post-restore 27.68M×2;
  NOP-game 9.2M).
- `FRAME_HARD_CAP` 50M → **150M** (3 frames, 1.81× measured);
  `LINUX_FRAME_HARD_CAP` 50M → **60M** (1 frame, 2.17×);
  `DETCHANNEL_FRAME_HARD_CAP` **unchanged** — verified synthetic-guest
  only (CreateVm + nanokernel `detchannel_frames_elf`, never the M9
  image), so it does not gate the real path. Derivation comments in the
  code; both ratios ≤4×; normal stop is BUDGET_REACHED.
- `linux_m5` frame-scheduling gate green **3 consecutive runs** with
  final caps, incl. both `VerifyReplay` bit-identical legs per run.
  Evidence: `target/frame-cap-retune-20260707T200907Z/` (rev + image
  identity + sha256s + logs). Fixture suites pass unchanged.
- Take-two ledger closed:
  `phase3-snapshot-restore-no-frame-under-no-tick-take-two/09-followup-resolution.md`.
- Discovered en route (filed as `determinism-hypervisor-jyo7`, P1): the
  `m5_net_loopback`/`m5_record_replay`-corpus/`m7_fork_verify` Linux
  paths are fixture-era stale (`PVBLKIO1` proof + old-initramfs
  `expected.txt` + corpus epoch_len overshoot) and unrunnable-green
  since `4b19c52` required the real-emulator initramfs. The
  net_loopback Run leg itself passes BudgetReached under the new cap.

## Item 2 — wall-clock backstop: CLOSED EMPIRICALLY, NOTHING IMPLEMENTED

- Probe A (idle `sti; hlt`, no tick, no event): exits to userspace,
  `GuestHalted`, prompt — no in-kernel irqchip means KVM cannot block
  HLT in-kernel. Probe B (MONITOR/MWAIT): retires as NOP → PAUSE spin →
  `HardCap` at exactly the cap. Both landed as permanent lab-lane
  regression tests in `runctl.rs` with 60s watchdogs.
- Resolution with the two answers, semantics note (`GuestHalted` on a
  NextSdkEvent run = "workload dead/parked" — the distinguishable signal
  the backstop would have provided), and the capture-watchdog
  non-applicability cite:
  `.agents/requests/nextsdkevent-run-wallclock-backstop/01-resolution.md`.
- **The bridge can retire its `timeout(1)` stopgap.**

## Beads

`determinism-hypervisor-ego1` (retune) and `-qq20` (backstop) closed
with resolution cites; `-2ng3` (handoff) open pending guest-sdk ack;
`-jyo7` (fixture-era Linux gates) filed for follow-up.
