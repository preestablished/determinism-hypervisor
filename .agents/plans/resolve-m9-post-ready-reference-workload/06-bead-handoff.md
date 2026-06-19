# Bead Handoff

## Suggested First Bead To Reopen Or Work

Start with `determinism-hypervisor-4s9.30` if the replacement fixture is
available, because `linux_worker_api` already has the strict manifest preflight
and will quickly prove whether the artifact contract is correct.

Start with `determinism-hypervisor-4s9.26` if the fixture is not yet available
but the next agent wants to work on direct VMM behavior first. That bead gives
the clearest low-level signal for post-READY execution and exact landing.

## Recommended Bead Notes When Starting

Append a note to the selected bead:

```text
Resuming from plan .agents/plans/resolve-m9-post-ready-reference-workload.
Goal is to replace or validate the M9 reference workload fixture so post-READY
Linux gates have real execution, frame marks, guest-driven IO, regions, and
VerifyReplay evidence. Do not weaken boot.toml or READY contract preflights.
```

## When To Close Each Bead

Close `4s9.30` only after:

- manifest/READY/region preflight has passed with the replacement fixture;
- `linux_worker_api` passes with `DH_M9_ALLOW_SKIP=0`;
- it proves region reads, fork, child run, and VerifyReplay;
- the known Linux VerifyReplay divergence is resolved or explicitly superseded
  by an accepted scope decision;
- the artifact hashes are in the bead note.

Close `4s9.24` only after:

- the CLI Linux gate runs 100 cold boots with `DH_M9_ALLOW_SKIP=0`;
- the PASS artifact includes READY identity and fixed post-READY budget state
  hash;
- no run is skipped.

Close `4s9.22` only after:

- the final artifact-backed Linux CLI gate runs with the staged M9 artifacts;
- Ready EventKind 14, not serial text, is observed;
- `dh-cli` still defaults to nanokernel when `--linux` is not used.

Close `4s9.26` only after:

- `linux_landing_counting` passes with `DH_M9_ALLOW_SKIP=0`;
- it reports at least 100 exact post-READY targets;
- it includes interrupt-adjacent coverage;
- it records zero overshoots/skips.

Close `4s9.25` only after:

- `linux_timer_determinism` passes 100 cases;
- delivered icounts and state hashes are identical;
- no forbidden host-time timer source is created or advertised.

Close `4s9.28` only after:

- Linux M4 transparency, frame scheduling, and net-or-pvblk IO regression tests
  pass;
- the tests use real frame marks and guest-driven IO in a replayed worker
  segment.

Close `4s9.27` only after:

- Linux M5 corpus replay has nonzero `EPOCH_HASH` verification;
- END state hash matches;
- no Divergence and no skips.

Close `4s9.29` only after:

- Linux M7 1000-child acceptance passes;
- cross-slot same-seed refs pass;
- nightly 100-child canary is wired if still in scope.
- test-list evidence proves Linux tests actually ran and did not fall back to
  nanokernel.

Close docs/final evidence beads only after the producer gates have actual
passing command evidence.

## If The Fixture Is External

If the reference-workload artifact builder is outside this repo and cannot be
modified by the implementation agent:

1. Keep the M9 producer beads blocked.
2. Add exact fixture requirements and current failing evidence to the relevant
   external issue or handoff, including the external repo path, issue ID, and
   expected artifact release SHA/hash.
3. Do not commit generated binary artifacts to this repo.
4. Keep this plan as the local acceptance checklist for the next attempt.

## Final Closeout Commands

After any implementation iteration:

```bash
bd ready
git status
git pull --rebase
bd dolt push
git push
git status
```

The final `git status` must say the branch is up to date with `origin/main`
and the working tree is clean.
