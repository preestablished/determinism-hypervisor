# Current State

## Bead Graph

`determinism-hypervisor-4s9.35` is manually marked `BLOCKED`, but the
dependencies listed by `bd show determinism-hypervisor-4s9.35` are closed.
Treat this as stale blocked state, not as a dependency blocker.

Before starting implementation, confirm with sequential `bd` commands because
the embedded Dolt backend only allows one writer at a time:

```bash
bd show determinism-hypervisor-4s9.35
bd show determinism-hypervisor-4s9
bd blocked
bd list --status blocked
```

Expected result before work:

- `4s9.35` remains blocked only by stale status and the absence of final
  suite evidence.
- `4s9` remains blocked because child `4s9.35` is not closed.
- `bd blocked` may report no dependency-blocked issues even while
  `bd list --status blocked` shows literal blocked statuses.

Move `4s9.35` into work explicitly:

```bash
bd update determinism-hypervisor-4s9.35 --status open \
  --append-notes "Unblocking stale blocked state: direct dependencies 4s9.30, 4s9.32, 4s9.33, and 4s9.34 are closed; starting final M9 acceptance evidence run on the reference KVM host."
bd update determinism-hypervisor-4s9.35 --claim
```

Do not mark the parent `4s9` open or closed until `4s9.35` is complete.

## Authority Documents

Use these local authorities. Do not infer the final suite from memory.

- `docs/ops/test-partitioning.md` - M9 artifact inputs, Linux gate classification, exact operator-run commands, and slot-core assumptions.
- `docs/ops/github-runner.md` - `kvm-intel` runner identity, artifact paths, one-KVM-job-at-a-time caveat, and security/scheduling context.
- `docs/phase-1-exit-gate.md` - current M9 Phase 1 rollup and expected Phase 1 Linux evidence shape.
- `docs/phase-2-exit-gate.md` - current M9 Phase 2 rollup and expected M4/M5/M7 evidence shape.
- `docs/upstream-divergences.md` - accepted M9 drift, including `/dev/vdb`, `base_image_hash`, READY EventKind 14, cmdline policy, Linux gate classification, artifact storage, and pv-blk loopback substitute.
- `.github/workflows/nightly-drift.yaml` - scheduled 100-child Linux M7 canary context.

## Existing Evidence Is Not Enough

The Phase 1 and Phase 2 docs already contain dated producer evidence from
the closed prerequisite beads. `4s9.35` is not asking for another planning
update. It asks for the complete final acceptance suite to pass as one
closeout step on the documented host.

The final closeout may reuse the same expected hashes and evidence patterns,
but it must record the fresh final run date and commands. If artifact bytes
have changed, record the new hashes with the final evidence and explain why
the change is authorized. If the artifact change is not authorized, stop and
file/update a bead instead of closing `4s9.35`.

## Acceptance Constraints

- Final evidence must never set `DH_M9_ALLOW_SKIP=1`.
- Final Linux M7 evidence must never set `DH_M7_ACCEPT_ALLOW_SKIP=1`.
- Missing artifacts are failed gates, not skips.
- Linux artifact-backed gates are operator-run on the reference host, not
  required CI.
- Full Linux M7 and cross-slot commands must run sequentially because they
  consume the same isolated slot-core set.
- The nanokernel/default gates remain separate coverage and must not be
  overwritten by Linux-only evidence.
