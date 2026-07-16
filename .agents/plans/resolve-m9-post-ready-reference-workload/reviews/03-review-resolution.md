# Review Resolution

Both subagents returned `REQUEST_CHANGES`. The plan was revised after their
reviews.

## Changes Applied

- Added `4s9.22` and `4s9.24` to primary blocked beads and handoff close
  criteria.
- Added explicit warning that replacing the smoke manifest is necessary but not
  sufficient to close `4s9.30` because Linux `VerifyReplay` has a known
  separate divergence.
- Added fixture-builder ownership boundaries: this repo owns validation and
  gates; the fixture builder owns Linux userspace, `/dev/vdb`, post-READY ABI,
  and artifacts.
- Added concrete fixture-builder discovery commands and requirement to record
  external repo/issue/release SHA if ownership is external.
- Split `linux_worker_api` into Phase A fixture evidence and Phase B full close
  evidence.
- Added a dedicated implementation phase for the known Linux `VerifyReplay`
  divergence.
- Added an explicit post-READY workload ABI: counting, interrupt, frame, IO, and
  region phases.
- Added universal nonzero Linux test-selection guard before accepting
  Linux-filtered worker/M7 commands as evidence.
- Added M7 cross-slot and nightly canary evidence.
- Added M5 corpus metadata requirements.
- Added authority references for `kvm.rs`, `config.rs`, and `proto_map.rs`.

## Remaining Judgment

The plan is still a plan, not an implementation. The next coding agent must
validate whether the fixture builder is local or external before reopening the
producer beads. If the builder is external and unavailable, keep the producer
beads blocked and use this plan as the local acceptance checklist.
