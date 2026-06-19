# Bead Closeout

Use this once both artifact-backed acceptance commands pass.

## If No Code Changed

If the only missing work was running the final artifact-backed gates, update and close the bead directly from clean `main`.

Recommended note content:

```text
Final artifact-backed acceptance passed with DH_M9_ALLOW_SKIP=0:
- cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
- cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture

Evidence: RestoreSnapshot, tier-A Fork, and VerifyReplay preserve Linux READY machine_config_hash/state_hash plus EVTC and BLKO sections, and boot_observer showed only the initial CreateVm BzImage load.
```

Commands:

```bash
bd update determinism-hypervisor-4s9.21 --append-notes '<paste final evidence>'
bd close determinism-hypervisor-4s9.21 --reason fixed
bd ready
bd dolt push
git status
git push
```

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
cargo build --workspace
```

Then run two independent review subagents against `main...HEAD`. Address any critical or important findings, rerun the relevant gates, commit review fixes if needed, and no-ff merge the iteration branch to `main`.

Final required push sequence:

```bash
git checkout main
git pull --ff-only origin main
git merge --no-ff <iteration-branch> -m 'ralph: iteration N merge - resolve-4s9-21-linux-boot-once-artifact-gate'
git push origin main
git branch -d <iteration-branch>
git push origin --delete <iteration-branch>
bd update determinism-hypervisor-4s9.21 --append-notes '<paste merge commit and gate evidence>'
bd close determinism-hypervisor-4s9.21 --reason fixed
bd dolt push
git push
git status
```

Do not leave a local or remote `ralph/iteration-*` branch behind.
