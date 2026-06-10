# Contributing

## The determinism gate is required for merge

Bit-identical replay is the product. The `kvm-intel` CI job runs the
live determinism suite on the baselined Intel box — including the M3
run-twice-compare regression (`tests/determinism/tests/regression.rs`:
1e9 instructions twice from cold boot, full state-hash chain compared)
and the §3.1 counting-semantics empirics — and is a **required status
check** on `main` (bead 8n7; IMPLEMENTATION-PLAN M3):

- Pull requests cannot merge until `kvm-intel` (and the hosted `host`
  matrix) pass.
- The check is enforced via branch protection
  (`required_status_checks`, non-strict). Administrators are exempt
  from push restrictions so the autonomous iteration flow can merge
  reviewed work directly; the protection is the floor for everyone
  and everything else.
- A red determinism job is NEVER worked around: a genuine divergence
  is a P0 (see `.agents/docs/MAP.md` conventions), and a
  counting-semantics failure on a new kernel/microcode triggers the
  documented BR_INST_RETIRED fallback decision — not a patch-around.

## Host drift

The kvm-intel runner's determinism class (CPU/microcode/kernel tuple)
is pinned in `ci/determinism-class.lock`. The `nightly-drift` workflow
compares the live host nightly and fails on drift; absorbing a
deliberate host change follows the re-baseline procedure in
`docs/ops/host-config-intel-box.md`.

## The usual gates

Everything that lands on `main` must pass, on both the hosted and the
kvm lanes:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check          # CI scopes to workspace members (sibling
                           # path deps are NOT formatted by this repo)
cargo test --workspace
```

The workspace also builds and is clippy-clean for
`aarch64-unknown-linux-gnu` (the arm lane tests the portable
determinism math; KVM modules are `cfg(target_arch = "x86_64")`-gated).
