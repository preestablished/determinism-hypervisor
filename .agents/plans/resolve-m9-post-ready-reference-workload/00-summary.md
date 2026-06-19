# Resolve M9 Post-READY Reference Workload

Plan name: `resolve-m9-post-ready-reference-workload`

Primary blocked beads:

- `determinism-hypervisor-4s9.22` - dh-cli Linux boot/run/gate final
  artifact-backed acceptance
- `determinism-hypervisor-4s9.24` - Phase 1 Linux gate with 100 cold boots
  and post-READY budget
- `determinism-hypervisor-4s9.26` - Linux landing precision and instruction counting
- `determinism-hypervisor-4s9.25` - Linux timer and IRQ determinism
- `determinism-hypervisor-4s9.27` - Linux M5 record/replay corpus
- `determinism-hypervisor-4s9.28` - Linux M4/M5 frame and IO regressions
- `determinism-hypervisor-4s9.30` - Linux worker API including regions and VerifyReplay

Downstream blocked beads:

- `4s9.29`, `4s9.31`, `4s9.32`, `4s9.33`, `4s9.34`, `4s9.35`

## Selected Blocker

The selected blocker is the M9 Linux reference workload/fixture contract. The
current staged `DH_M9_INITRAMFS` is the M2 smoke image. It emits a READY event
and then terminates, which is enough for `linux_ready` but not enough for the
M9 post-READY gates.

The replacement fixture must:

- reach guest-sdk `Ready` EventKind 14 through detchannel, not serial text;
- keep executing after READY in a deterministic workload;
- expose exact retired-instruction landing targets after READY;
- expose an interrupt window usable by scheduled timer/IRQ delivery;
- emit pv-pad `FRAME_MARK` records;
- perform guest-driven deterministic IO through the selected M9 `/dev/vdb`
  contract, or through an explicitly accepted replacement fixture;
- declare the reference-workload `boot.toml` control and region contract that
  `crates/dh-worker/tests/linux_worker_api.rs` already preflights.

## Why This Is The Right Root

Most remaining M9 beads are not independent code bugs. They are evidence gates
that require a Linux guest workload which exists beyond READY. The current
fixture fails that shape:

- `4s9.26` observed first post-READY landing targets overshoot or free-run to
  terminal HLT.
- `4s9.25` uses the same `TimerArm -> agenda -> land_at -> inject_at_boundary`
  path, so it cannot honestly prove Linux timer delivery from the current READY
  stop.
- `4s9.28` needs post-READY frame marks and guest-driven IO; the current fixture
  halts before either.
- `4s9.30` currently fails before KVM because `boot.toml` lacks
  `[unit.control]`, `refwork-ctl`, `game_dev = "/dev/vdb"`, and
  `[[expected_region]]` entries.
- `4s9.30` also has a separate known Linux `VerifyReplay` divergence after the
  manifest preflight is fixed. Replacing the smoke fixture is necessary but not
  sufficient to close that bead.

Do not close these beads by weakening tests to accept boot-to-READY only. The
work is to provide or build the correct fixture and then wire tests around that
fixture.

Every Linux-filtered worker/M7 command must prove it selected and executed at
least one Linux test. A command that exits successfully because the filter
selected zero tests is not evidence.

## Plan Files

- `01-authority-and-current-state.md` records the source documents, current
  blocked evidence, and repo seams.
- `02-fixture-contract.md` defines the required Linux artifact behavior.
- `03-implementation-sequence.md` orders implementation work for another agent.
- `04-test-and-acceptance-gates.md` names the gates to run and what each must
  prove.
- `05-risks-and-debugging.md` lists expected failure modes and how to triage
  them without weakening acceptance.
- `06-bead-handoff.md` tells the implementation agent how to update Beads when
  the plan is complete or partially complete.
- `reviews/` contains the two independent subagent reviews and the resolution
  notes applied to this plan.
