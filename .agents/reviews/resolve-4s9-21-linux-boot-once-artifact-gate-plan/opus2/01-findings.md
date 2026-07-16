## Important

1. Mandatory push protocol is incomplete in both closeout paths.

References:
- `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:19`
- `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:49`
- `AGENTS.md:68`

The no-code closeout sequence runs `bd dolt push`, `git status`, then `git push`, but
it omits the required final `git pull --rebase` before pushing and puts `git status`
before the push instead of after it. The code-change closeout has the same problem
after bead closure: it pushes `main` before updating/closing the bead, then runs
`bd dolt push`, `git push`, `git status` without the required final
`git pull --rebase`. `AGENTS.md` requires the end-of-session sequence
`git pull --rebase`, `bd dolt push`, `git push`, then `git status` showing the branch
is up to date, and also requires retrying push failures.

Impact: a future agent could correctly run the artifact gates and close `4s9.21`, but
still leave Git or bead state stranded locally or fail to verify that the branch is up
to date with origin. This directly misses the requested push requirements.

Expected correction: both closeout paths should end with the repository-mandated
sequence after all bead updates and branch cleanup:

```bash
git pull --rebase
bd dolt push
git push
git status  # MUST show "up to date with origin"
```

They should also say to resolve and retry if either push fails.

## Medium

2. The artifact-only path skips the normal bd claim step.

References:
- `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/02-acceptance-runbook.md:9`
- `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:19`
- `AGENTS.md:7`
- `AGENTS.md:46`

The happy-path runbook starts with `bd ready` and `bd show`, and the no-code closeout
updates and closes the bead directly. It never tells the future agent to claim
`determinism-hypervisor-4s9.21` before doing the final artifact-gate work. The local
quick reference includes `bd update <id> --claim` as the atomic work-claim step, and
`bd show 4s9.21` currently reports the bead assigned to Matt Spurlin.

Impact: this does not weaken the technical acceptance evidence, but it can violate the
bd workflow and make ownership ambiguous when an agent closes the bead after running
the real artifacts.

Expected correction: the start state should include either:

```bash
bd update determinism-hypervisor-4s9.21 --claim
```

or an explicit instruction that the current human owner is running the gate and should
keep assignment unchanged.

## No Findings

- Exact acceptance gates: the plan's two `DH_M9_ALLOW_SKIP=0 cargo test ... linux_boot_once ... --ignored --nocapture` commands match the comments in `crates/dh-worker/tests/restore_engine.rs` and `crates/dh-worker/tests/replay_engine.rs`.
- Artifact preconditions: the required `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, `DH_M9_GAME_IMAGE`, `DH_M9_IMAGE_CACHE`, KVM, dirty-ring, and `DH_M9_ALLOW_SKIP=0` conditions are present.
- Ralph fallback: if code changes are needed, the plan requires a Ralph iteration branch, focused fix, two independent review subagents, review fixes, no-ff merge, and push.
