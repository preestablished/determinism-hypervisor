# Findings

## Critical Findings

None.

## Important Findings

1. Artifact preflight is too weak for the image cache write path.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/01-artifact-prerequisites.md:20`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/01-artifact-prerequisites.md:38`
   - `crates/dh-worker/tests/common/mod.rs:283`
   - `crates/dh-worker/tests/common/mod.rs:359`

   The plan says the cache path only needs to be a readable directory and the preflight only runs `test -d "$DH_M9_IMAGE_CACHE"`. The acceptance helper calls `populate_m9_image_cache`, which hard-links or copies artifacts into that directory before building the worker config. A readable but non-writable or stale cache can pass the plan preflight and then fail inside the expensive artifact-backed tests. Require `DH_M9_IMAGE_CACHE` to be writable, or require all four lowercase BLAKE3 cache entries to exist and hash-match before running the final gates.

2. The runbook can misclassify bad M9 artifacts as product failures.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/03-failure-triage.md:19`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/03-failure-triage.md:26`
   - `docs/ops/test-partitioning.md:58`
   - `docs/ops/test-partitioning.md:70`
   - `crates/dh-worker/tests/linux_worker_api.rs:996`

   `Run until Ready` not reaching EventKind 14 is listed as a product failure, but the existing ops docs say the M9 initramfs must be the reference-workload image with a specific `boot.toml` contract, and `linux_worker_api::pvblk_dev_vdb` asserts that contract. If the agent points at the M2 smoke initramfs or an incomplete reference workload, the two target tests can fail before READY and the plan steers the agent toward code investigation. Add an artifact-validation step or at least tell the agent to run the existing ignored `linux_worker_api` artifact contract gate before treating a READY timeout as a restore/replay product regression.

3. KVM host preflight should be explicit, not implicit in the first long test.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/01-artifact-prerequisites.md:28`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/01-artifact-prerequisites.md:36`
   - `docs/ops/github-runner.md:24`
   - `docs/ops/github-runner.md:30`
   - `crates/dh-worker/src/preflight.rs:328`

   The plan names Linux x86_64, KVM, and dirty-ring support, but its preflight commands do not run the existing host preflight. `dh-workerd --preflight` checks the ARCH 7.4 host and KVM 2.1 requirements and constructs a real slot VM. Add `cargo run -p dh-worker --bin dh-workerd -- --preflight` before the artifact gates so `/dev/kvm` permissions, dirty-ring support, kvm_intel settings, THP, and related host drift fail before the M9 Linux tests spend time booting.

4. Ralph verification is under-specified for determinism-sensitive code fixes.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/02-acceptance-runbook.md:68`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/02-acceptance-runbook.md:77`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:36`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:43`

   If the artifact-backed tests fail and the agent changes restore, fork, replay, runtime, or state-hash code, the plan only asks for one `cargo test --workspace` run. The project Beads memory for Ralph explicitly calls out determinism/hash-sensitive changes as requiring 3 or more consecutive full workspace runs, and the current bead notes for the already-merged work used `cargo test --workspace x3 consecutive`. Update the code-change path to require the three consecutive full workspace runs before review/merge.

5. Closeout push ordering does not fully match the repo's mandatory Beads session protocol.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:19`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:25`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:26`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:49`
   - `AGENTS.md:68`

   The no-code closeout omits the required final `git pull --rebase` before `bd dolt push`, runs `git status` before the final `git push`, and does not state that final status must show "up to date with origin". For the code-change path, the plan correctly uses `git pull --ff-only` before the no-ff merge, but it should preserve the Ralph rule not to rebase after a merge and still finish with `bd dolt push`, `git push`, and a final `git status` showing the branch is up to date. As written, another agent can follow the commands and still miss the documented closeout invariant.

6. The code-change path does not require re-validating the actual merged result if `main` moved.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:47`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:52`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:54`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:55`

   The plan runs gates on the iteration branch, then updates `main` and creates a merge commit. If `main` advanced during the artifact run or reviews, the tested branch result is not necessarily the pushed merge result. Require either refreshing the branch before the expensive gates or running the relevant acceptance/workspace gates on the no-ff merge result before pushing `main`.

## Minor Suggestions

1. Capture final acceptance logs and host/artifact evidence explicitly.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/02-acceptance-runbook.md:30`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:11`

   The commands are exact, but the plan does not say where to keep stdout/stderr, host preflight output, `git rev-parse HEAD`, or artifact fingerprints. Adding a simple `tee` convention would make the bead note defensible and would help if only one of the two artifact-backed tests fails.

2. Avoid shell-quoting multi-line evidence directly in `bd update --append-notes`.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/05-bead-closeout.md:22`
   - `bd update --help`

   `--append-notes` is a valid flag, but a pasted multi-line shell single-quoted string is easy to break if the evidence contains quotes. Prefer `bd note determinism-hypervisor-4s9.21 "$(cat <evidence-file>)"` or a short manually quoted summary plus a path to the preserved evidence.

3. Include the existing ops artifact docs by reference.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/01-artifact-prerequisites.md:53`
   - `docs/ops/test-partitioning.md:44`
   - `docs/ops/github-runner.md:60`

   The plan duplicates some artifact requirements but omits several operational details already documented in `docs/ops/test-partitioning.md` and `docs/ops/github-runner.md`. Link those docs directly so the next agent has the canonical staging layout and runner assumptions.

4. Failure triage should say what to do if infrastructure cannot be fixed in-session.

   References:
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/03-failure-triage.md:7`
   - `.agents/plans/resolve-4s9-21-linux-boot-once-artifact-gate/03-failure-triage.md:17`

   "Fix the host/artifact setup and rerun" is directionally right, but it does not tell an agent to append failure notes to `4s9.21`, keep the bead blocked, and file or update follow-up work when the blocker is outside the coding session.

