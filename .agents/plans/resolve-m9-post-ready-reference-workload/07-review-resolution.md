# Review Resolution

Two subagents reviewed this plan and both returned `REQUEST_CHANGES`. The plan
has been revised to address their findings.

## Changes Applied

- Added `4s9.22` and `4s9.24` to primary blocked beads and close criteria.
- Made `4s9.30` explicitly depend on both fixture replacement and the known
  Linux `VerifyReplay` divergence investigation.
- Added fixture-builder ownership boundaries: this repo owns validation and
  gates; the fixture builder owns Linux userspace, `/dev/vdb`, post-READY ABI,
  and generated artifacts.
- Added concrete fixture-builder discovery commands and the requirement to
  record the external repo, issue, and release SHA when ownership is external.
- Split `linux_worker_api` acceptance into Phase A fixture evidence and Phase B
  full close evidence.
- Added a dedicated implementation phase for the known Linux `VerifyReplay`
  divergence.
- Added a concrete post-READY workload ABI for counting, interrupt, frame, IO,
  and region phases.
- Added a universal nonzero Linux test-selection guard before accepting
  Linux-filtered worker or M7 commands as evidence.
- Added M7 cross-slot and nightly canary evidence.
- Added M5 corpus metadata requirements.
- Added authority references for KVM, config, and worker protocol mapping
  seams.

## Remaining Judgment

The next coding agent must validate whether the fixture builder is local or
external before reopening the producer beads. If the builder is external and
unavailable, keep the producer beads blocked and use this plan as the local
acceptance checklist.
