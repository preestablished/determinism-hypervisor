# Acceptance Completeness Review

Reviewer: `019ee59d-ef91-7bb3-95fc-d1230f57f24f` (`Confucius`)

Status: request changes

## Findings

- High: The plan incorrectly required Linux child `RunResponse.icount <= M9_LINUX_CHILD_HARD_CAP`. `RunResponse.icount` is the absolute stop boundary, so after READY it can exceed a 5M post-READY cap. The plan should check `BudgetReached`, `frames_elapsed == 5`, DHILOG header end counters against segment counters, and optionally `run.icount - ready_icount <= hard_cap`.

- Medium: The frame-mark checks were too loose. Instead of "at least" five marks and generally increasing frame indices, acceptance should require exactly `M9_LINUX_CHILD_FRAMES` marks, expected frame indices `ready_frame_counter + 1..=child.frame_counter`, strictly increasing frame-mark icounts, and `child.frame_counter == ready_frame_counter + M9_LINUX_CHILD_FRAMES`.

- Medium: The final full commands relied on defaults. The acceptance plan should make `DH_M7_ACCEPT_JOBS=1000` and `DH_M7_CROSS_CHECKS=10` explicit and include a separate full cross-slot command, not only a two-child smoke.

- Medium: The reference-host preflight should be stricter. It should require `bash ci/check-determinism-class.sh`, the Linux fixture contract gate, and an affinity check proving `DH_M7_ACCEPT_SLOT_CORES=2-5` is available. The current shell may expose only cores `0-1`, so the plan should tell the implementer to run acceptance under the self-hosted runner or cpuset that exposes cores `2-5`.

- Medium: The nightly workflow plan should update `alert-on-failure.needs` and the alert issue body/title. Otherwise a failing Linux canary may not create the visible nightly failure issue.

## Overall

The selected bead and general strategy match the acceptance criteria, but the plan needed tighter counter-domain, frame-mark, final-command, and nightly-alert requirements before another coding agent could implement it safely.
