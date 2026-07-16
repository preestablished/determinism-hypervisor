# Verification Notes

## Commands Run

- `bd prime`
- `bd show determinism-hypervisor-4s9.22`
- `git status --short --branch`
- `git diff --stat main...HEAD`
- `git diff --check main...HEAD`
- `rg -n "dh_worker|dh-worker|image_resolver|ops|linux|bzimage|initramfs|base_image|game_image|cmdline|EventKind|Ready|gate|run|boot" tools/dh-cli/src tools/dh-cli/tests -S`
- `cargo tree -p dh-cli | rg "dh-worker|dh_worker"`
- `cargo test -p dh-cli --tests`
- `cargo run -p dh-cli -- gate --runs 2`
- `printenv DH_M9_BZIMAGE DH_M9_INITRAMFS DH_M9_BASE_IMAGE DH_M9_GAME_IMAGE DH_M9_IMAGE_CACHE DH_M9_ALLOW_SKIP`

## Results

- `git diff --check main...HEAD`: passed.
- `cargo test -p dh-cli --tests`: passed.
  - `src/lib.rs`: 14 passed.
  - `src/main.rs`: 0 tests.
  - `tests/boot_hello.rs`: 6 passed.
  - `tests/cli_args.rs`: 8 passed.
  - `tests/skid_gate.rs`: 2 passed.
- `cargo run -p dh-cli -- gate --runs 2`: passed and printed nanokernel `plain-landing` and `timer-event` PASS artifacts followed by `PHASE-1 DETERMINISM GATE: PASS (2 runs each)`.
- `cargo tree -p dh-cli | rg "dh-worker|dh_worker"`: no matches, so the crate dependency tree does not include `dh-worker`.
- `printenv DH_M9_*`: no output, exit 1. The artifact-backed Linux gate was not run because this environment lacks the required M9 artifact paths.

## Static Review Notes

- `dh-cli` routes Linux `boot`, `run`, and `gate` through the new direct harness in `tools/dh-cli/src/linux.rs`, not through `tools/dh-cli/src/ops.rs`.
- The direct harness uses `dh-vmm`/`dh-devices` directly and does not import `dh-worker` or `dh_worker`.
- The CLI Linux bus matches the M9 worker device set shape for this milestone: detchannel, pv-clock, pv-pad, entropy, pv-blk at `0xD000_4000`, and debug serial.
- The `--base-image`/`--game-image` split matches existing worker M9 tests: `DH_M9_BASE_IMAGE` is fixture context while `DH_M9_GAME_IMAGE` is the current pv-blk backing and `MachineConfig.base_image_hash`.

