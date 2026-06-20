# Review Resolution

Two subagents reviewed this plan on 2026-06-20.

## Reviewers

- `019ee59d-d8ce-7ed0-b26f-99c8825793cd` (`Socrates`): implementation feasibility and codebase-fit review.
- `019ee59d-ef91-7bb3-95fc-d1230f57f24f` (`Confucius`): acceptance completeness and reference-host review.

Both reviewers found the plan directionally correct and requested changes before handoff.

Full review notes:

- `08-review-feasibility.md`
- `09-review-acceptance.md`

## Changes Applied

- Corrected the counter model. The plan now distinguishes worker cumulative counters from DHILOG segment counters and requires Linux child segment counters to be computed from the READY root cumulative counters.
- Replaced the flat `AcceptanceHarness` sketch with an enum-backed harness so the Linux path can retain `common::M9LinuxReady` without duplicating ownership of service, store, tempdir, runtime, or lease resources.
- Made `ParsedChildLog` concrete and added required fields for header identity, canonical count, epoch hashes, frame marks, log hash, and end counters.
- Tightened Linux frame-mark validation from "at least N marks" to exactly `M9_LINUX_CHILD_FRAMES`, with expected absolute frame indices `ready_frame_counter + 1..=ready_frame_counter + M9_LINUX_CHILD_FRAMES` and strictly increasing frame-mark icounts.
- Made full acceptance commands explicit with `DH_M7_ACCEPT_JOBS=1000` and `DH_M7_CROSS_CHECKS=10` to avoid stale environment overrides producing weak evidence.
- Added a separate full cross-slot command to the validation plan.
- Strengthened reference-host preflight with `bash ci/check-determinism-class.sh`, the Linux fixture contract gate, and an explicit affinity check proving cores `2-5` are available to the acceptance process.
- Added nightly alert-job requirements: include the new Linux M7 canary in `alert-on-failure.needs` and update the alert issue text.
- Changed the baseline guard command from `--list` to an executing ignored guard run that is expected to fail before implementation.

## Remaining Judgment

The plan still leaves one implementation tradeoff to the coding agent: whether to validate the Linux `PVBLKIO1` meta proof for every one of the 1000 children or for a bounded subset plus all cross-slot children. That is intentional because per-child guest-memory reads may affect runtime. The non-negotiable Linux evidence remains frame marks, epoch hashes, lineage, zero `Divergence`, and matching `VerifyReplay.Done.end_state_hash` for every child.

## Fresh Review Follow-Up

Two fresh subagent reviews were run after the initial plan was committed. I agree with the concrete findings and incorporated them:

- The unfiltered full Linux M7 acceptance command now includes `--test-threads=1`, and the plan explicitly requires full acceptance and targeted cross-slot evidence to run sequentially on the reference host.
- The nightly Linux canary instructions now say to duplicate the existing M7 job's repository checkout, sibling dependency checkout, Rust toolchain, KVM, and `nasm` setup before adding the Linux-specific environment and command.
- The closeout runbook now includes `git add`, `git commit`, exact commit SHA capture, and a Beads evidence comment before `bd close`.
- The existing explicit-core helper guidance was already source-compatible with current M9 helper users; keep that compatibility when implementing.
