# 4s9.33 Gate Docs And Classification

## Objective

Update operational docs and workflow classification for M9 Linux gates after the Linux implementation beads have landed. Close `4s9.33` only when docs, runner requirements, and CI/nightly/operator classification agree.

## Files To Inspect

- `docs/ops/test-partitioning.md`
- `docs/ops/github-runner.md`
- `.github/workflows/ci.yaml`
- `.github/workflows/nightly-drift.yaml`
- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`

The first four are the primary edit targets from the bead. The phase docs are consumers and should be referenced for consistency, but avoid doing `4s9.32` evidence work here unless the user explicitly expands scope.

## Classification Model

Use these categories consistently:

- **Required CI:** short enough for PR/main merge checks; no external staged artifact dependency unless the runner already guarantees it. Current examples: hosted workspace checks and kvm-intel non-ignored workspace checks.
- **Nightly:** scheduled drift/canary checks on the self-hosted runner. Current examples: determinism class, run-twice canary, checked-in nanokernel M5 corpus, nanokernel M7 100-child canary, Linux M7 100-child canary, DHILOG fuzz.
- **Operator-run:** long acceptance, full Linux evidence, staged M9 artifact gates, full M7 1000-child/cross-slot evidence, throughput soak, and any command requiring deliberate exclusive host scheduling.

Do not promote full Linux M9 acceptance commands to required CI unless runtime and artifact provisioning are explicitly accepted.

## Commands And Env Vars To Ensure Are Documented

Before editing command tables, audit the producer bead evidence and current test targets:

```bash
bd show determinism-hypervisor-4s9.22
bd show determinism-hypervisor-4s9.24
bd show determinism-hypervisor-4s9.25
bd show determinism-hypervisor-4s9.26
bd show determinism-hypervisor-4s9.27
bd show determinism-hypervisor-4s9.28
bd show determinism-hypervisor-4s9.29
bd show determinism-hypervisor-4s9.30
```

Use these records to cite passing no-skip Linux evidence in the `4s9.33` close comment. Do not rely only on the three direct dependencies listed on `4s9.33`; the docs classify the whole M9 gate surface.

`docs/ops/test-partitioning.md` should include exact command rows for:

- Linux fixture contract:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p determinism-tests --test linux_fixture_contract -- --ignored --nocapture
  ```

- Linux boot-to-READY identity:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture
  ```

- Linux Phase 1 CLI gate:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo run -p dh-cli -- gate --linux --runs 100 \
    --bzimage "$DH_M9_BZIMAGE" \
    --initramfs "$DH_M9_INITRAMFS" \
    --base-image "$DH_M9_BASE_IMAGE" \
    --game-image "$DH_M9_GAME_IMAGE"
  ```

- Linux landing/counting:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture
  ```

- Linux timer/IRQ determinism if present in current test targets:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture
  ```

- Linux M4/M5 frame/net regressions:

  ```bash
  DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
  cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture
  ```

  ```bash
  DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
  cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture
  ```

  ```bash
  DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
  cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture
  ```

- Linux M5 corpus reverify:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p dh-worker --test m5_record_replay --release \
    linux_m5_record_replay_post_ready_corpus_reverifies \
    -- --ignored --nocapture
  ```

- Linux worker API:

  ```bash
  DH_M9_ALLOW_SKIP=0 \
  cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture
  ```

- Linux M7 nightly 100-child canary and full M7 operator commands, preserving the rows added by `4s9.29`.
- Nanokernel/default M7 rows, preserving existing coverage.

Every Linux artifact command should mention the required `DH_M9_*` env vars:

```bash
DH_M9_BZIMAGE
DH_M9_INITRAMFS
DH_M9_BASE_IMAGE
DH_M9_GAME_IMAGE
DH_M9_IMAGE_CACHE
DH_M9_ALLOW_SKIP=0
```

## Runner Docs

Update `docs/ops/github-runner.md` if needed so it states:

- artifact staging path used by nightly Linux jobs;
- `DH_M9_IMAGE_CACHE` must exist and be writable;
- Linux M7 nightly uses `taskset -c "$DH_M7_ACCEPT_SLOT_CORES"` and default slot cores `2-5`;
- full M9 acceptance and full M7 cross-slot gates are operator-run, not scheduled nightly;
- existing nanokernel M7 nightly remains live;
- the single `kvm-intel` runner serializes KVM jobs and long operator dispatches can delay nightly.

## Workflow Audit

Check `.github/workflows/ci.yaml`:

- It should preserve hosted and kvm-intel workspace gates.
- It should not run full artifact-backed Linux acceptance unless explicitly classified as required.
- It must keep fork-PR protection for self-hosted runner jobs.

Check `.github/workflows/nightly-drift.yaml`:

- It should include `m7-fork-verify-100` for nanokernel/default M7.
- It should include `m7-linux-fork-verify-100` for Linux M7 with M9 artifact env vars and no-skip env.
- It should preflight `/dev/kvm`, `nasm`, artifact paths, image cache, and slot affinity.
- `alert-on-failure.needs` should include the Linux M7 canary.
- Alert title/body should mention Linux M7 canary failures.

If the workflows already satisfy a requirement, leave them unchanged and document that in the bead evidence comment.

## Close Criteria

Close `4s9.33` only when:

- producer bead evidence for `4s9.22` through `4s9.30` has been reviewed and the close comment cites the no-skip evidence backing the documented Linux commands;
- docs and workflow classification are consistent;
- exact Linux commands and env vars are documented;
- runner requirements include artifact staging and slot affinity;
- nanokernel lanes are preserved;
- YAML parses;
- `rg` checks find the expected Linux M9/M7 env vars and classifications in docs/workflows;
- `git diff --check` passes.
