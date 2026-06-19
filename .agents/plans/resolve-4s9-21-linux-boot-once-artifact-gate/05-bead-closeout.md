# Bead Closeout

Use this once both artifact-backed acceptance commands pass.

## If No Code Changed

If the only missing work was running the final artifact-backed gates, update and close the bead directly from clean `main`.

Recommended note content:

```bash
cat > /tmp/4s9.21-evidence/bd-notes.txt <<'EOF'
Final artifact-backed acceptance passed with DH_M9_ALLOW_SKIP=0:
- cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
- cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture

Evidence: RestoreSnapshot, tier-A Fork, and VerifyReplay preserve Linux READY machine_config_hash/state_hash plus EVTC and BLKO sections, and boot_observer showed only the initial CreateVm BzImage load.
Logs:
- /tmp/4s9.21-evidence/restore_engine-linux_boot_once.log
- /tmp/4s9.21-evidence/replay_engine-linux_boot_once.log
EOF
```

Commands:

```bash
bd update determinism-hypervisor-4s9.21 --claim
bd update determinism-hypervisor-4s9.21 --append-notes "$(cat /tmp/4s9.21-evidence/bd-notes.txt)"
bd close determinism-hypervisor-4s9.21 --reason fixed
bd ready
git pull --rebase
bd dolt push
git push
git status
```

If either `bd dolt push` or `git push` fails, resolve the failure and retry
until the push succeeds. Do not leave completed bead or code state stranded
locally.

After closing `4s9.21`, expect `bd ready` to surface downstream Linux M9 beads that were blocked by it, especially `4s9.23`.

## If Code Changed

Follow the full Ralph closeout:

```bash
cargo fmt
cargo test -p dh-worker --test restore_engine linux_boot_once --release --no-run
cargo test -p dh-worker --test replay_engine linux_boot_once --release --no-run
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture
git diff --check
cargo test --workspace
cargo test --workspace
cargo test --workspace
cargo build --workspace
```

Then run two independent review subagents against `main...HEAD`. Address any critical or important findings, rerun the relevant gates, commit review fixes if needed, and no-ff merge the iteration branch to `main`.

After merging, rerun at least the two artifact-backed acceptance commands on
the actual merged `main`. If `main` moved before the merge, also rerun the
three consecutive workspace tests on merged `main` before closing the bead.
Before the `bd update` step, create or update
`/tmp/4s9.21-evidence/bd-notes.txt` with the merge commit, the exact gate
commands, and the evidence log paths.

Final required push sequence:

```bash
git checkout main
git pull --ff-only origin main
git merge --no-ff <iteration-branch> -m 'ralph: iteration N merge - resolve-4s9-21-linux-boot-once-artifact-gate'
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture
git push origin main
git branch -d <iteration-branch>
git push origin --delete <iteration-branch>
bd update determinism-hypervisor-4s9.21 --claim
bd update determinism-hypervisor-4s9.21 --append-notes "$(cat /tmp/4s9.21-evidence/bd-notes.txt)"
bd close determinism-hypervisor-4s9.21 --reason fixed
bd ready
git pull --rebase
bd dolt push
git push
git status
```

If any push fails, resolve the failure and retry until it succeeds. Final
`git status` must report `main` up to date with `origin/main` and a clean
worktree.

Do not leave a local or remote `ralph/iteration-*` branch behind.
