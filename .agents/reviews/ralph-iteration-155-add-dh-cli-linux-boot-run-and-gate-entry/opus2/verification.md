# Verification Notes

Reviewed branch:

- `ralph/iteration-155-add-dh-cli-linux-boot-run-and-gate-entry`
- HEAD `b2dde45 ralph: iteration 155 checkpoint - dh-cli linux ready gate`
- Base `e32a55c ralph: iteration 154 merge - ensure-linux-restore-fork-and-replay-neve`

Commands run:

- `bd prime`
- `bd show determinism-hypervisor-4s9.22`
- `git diff --stat main...HEAD`
- `git diff --name-only main...HEAD`
- `cargo test -p dh-cli --tests`
- `cargo run -p dh-cli -- gate --runs 2`
- `printenv DH_M9_BZIMAGE DH_M9_INITRAMFS DH_M9_BASE_IMAGE DH_M9_GAME_IMAGE DH_M9_IMAGE_CACHE DH_M9_ALLOW_SKIP`
- `cargo tree -p dh-cli --edges normal | rg -n "dh-worker|dh_worker|dh-cli|dh-vmm|dh-devices|detguest|dh-inputlog|dh-verify"`
- `git diff --check main...HEAD`

Results:

- `cargo test -p dh-cli --tests` passed: 14 unit tests, 6 `boot_hello` tests, 8 `cli_args` tests, and 2 `skid_gate` tests.
- `cargo run -p dh-cli -- gate --runs 2` passed and printed nanokernel `plain-landing` and `timer-event` PASS artifacts.
- `DH_M9_*` artifact environment variables were not set, so I did not run the artifact-backed Linux gate command.
- `cargo tree`/grep showed no `dh-worker` normal dependency from `dh-cli`; `dh-cli` depends on `dh-vmm`, `dh-devices`, `dh-inputlog`, `dh-verify`, `detguest-host`, and `detguest-wire`.
- `git diff --check main...HEAD` passed.

Notes:

- Initial parallel `bd show` hit the embedded Dolt lock; a later sequential `bd show determinism-hypervisor-4s9.22` succeeded.
