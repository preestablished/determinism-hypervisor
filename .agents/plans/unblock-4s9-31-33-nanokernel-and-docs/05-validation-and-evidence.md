# Validation And Evidence

## Shared Preflight

Run before either bead:

```bash
git status --short --branch
bash ci/check-determinism-class.sh
test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm
which nasm
```

If Linux artifact docs or commands are validated:

```bash
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$m9_artifact_root/bzImage"
export DH_M9_INITRAMFS="$m9_artifact_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
export DH_M9_ALLOW_SKIP=0
mkdir -p "$DH_M9_IMAGE_CACHE"
test -f "$DH_M9_BZIMAGE"
test -f "$DH_M9_INITRAMFS"
test -f "$DH_M9_BASE_IMAGE"
test -f "$DH_M9_GAME_IMAGE"
test -d "$DH_M9_IMAGE_CACHE"
b3sum "$DH_M9_BZIMAGE" "$DH_M9_INITRAMFS" "$DH_M9_BASE_IMAGE" "$DH_M9_GAME_IMAGE"
```

Confirm slot-core availability for commands documented as `2-5`:

```bash
taskset -c 2-5 true
taskset -c 2-5 cat /proc/self/status | rg '^Cpus_allowed_list:'
```

## 4s9.31 Evidence Summary Shape

Record a Beads comment like:

```text
Nanokernel preservation complete on <host>.

Commit: <sha>
Host: <host>; determinism-class lock matched: <summary>.

Commands:
- cargo test --workspace: PASS, <duration if available>
- cargo run -p dh-cli -- gate --runs 100: PASS, still nanokernel/default, <key output>
- cargo test -p determinism-tests --test ...: PASS
- cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture: PASS
- cargo test -p dh-worker --test m7_fork_verify -- --nocapture: PASS
- <nanokernel M7 ignored smoke command>: PASS, verified=<n>, divergence=0, unique_hashes=<n>

Fixtures:
tests/nanokernel/** unchanged.
crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s unchanged.

Docs:
Updated <files> with dated post-M9 nanokernel preservation evidence.
```

## 4s9.33 Evidence Summary Shape

Record a Beads comment like:

```text
Linux gate docs and runner classification complete.

Commit: <sha>

Producer evidence reviewed:
- 4s9.22: <CLI Linux evidence summary>
- 4s9.24: <Phase 1 Linux gate evidence summary>
- 4s9.25: <Linux timer/IRQ evidence summary>
- 4s9.26: <Linux landing/counting evidence summary>
- 4s9.27: <Linux M5 corpus evidence summary>
- 4s9.28: <Linux M4/M5 frame/net evidence summary>
- 4s9.29: <Linux M7 full/cross-slot/nightly evidence summary>
- 4s9.30: <Linux worker API evidence summary>

Docs:
- docs/ops/test-partitioning.md lists Linux M9 artifact env vars, fixture contract, Phase 1 Linux gate, landing/counting, timer/IRQ if present, M4/M5 frame/net regressions, M5 corpus, Linux M7 nightly/full/cross-slot, and nanokernel/default M7 commands.
- docs/ops/github-runner.md records M9 artifact staging, image-cache requirement, slot-core/taskset requirement, and nightly/operator split.

Workflows:
- .github/workflows/ci.yaml preserves required non-ignored workspace gates and fork-PR self-hosted protection.
- .github/workflows/nightly-drift.yaml has nanokernel M7 and Linux M7 100-child canaries; alert-on-failure includes Linux M7.

Validation:
- python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/nightly-drift.yaml")); yaml.safe_load(open(".github/workflows/ci.yaml"))'
- rg checks for DH_M9_ALLOW_SKIP, DH_M7_ACCEPT_GUEST, M7 Linux, operator-run, nightly.
- git diff --check.
```

## Quality Gates Before Commit

For doc-only changes:

```bash
python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yaml")); yaml.safe_load(open(".github/workflows/nightly-drift.yaml"))'
rg -n "DH_M9_ALLOW_SKIP|DH_M9_BZIMAGE|DH_M7_ACCEPT_GUEST|M7 Linux|operator-run|nightly" \
  docs/ops/test-partitioning.md docs/ops/github-runner.md .github/workflows/ci.yaml .github/workflows/nightly-drift.yaml
git diff --check
```

For any Rust/test command evidence:

```bash
cargo fmt --check
git diff --check
```

Do not run full M9 acceptance for `4s9.31` or `4s9.33` unless the user explicitly asks. That belongs to `4s9.35`.
