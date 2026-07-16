# Review Overview — iteration 54 phase1 docs

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-54-phase1-docs vs main
- **Scope:** DOCUMENTATION-ONLY (beads b0h + hny). Two files:
  - `docs/ops/test-partitioning.md` (NEW) — host-runnable vs kvm-gated test matrix + condensed Intel-box runbook
  - `README.md` (appended) — dh-cli subcommand reference, measured numbers, R2 status, doc links
- **Method:** Adversarial. Every documented command executed verbatim on the
  lab box (i5-8400, kernel 6.8.0-124, ucode 0xfa, /dev/kvm rw). Every number
  cross-checked against source/decision docs.

## Verdict: APPROVE

The docs are operationally faithful. Every command runs as written and exits 0.
Every cited number checks out against source. The R2 narrative matches the
vendored ARCH §3.1 text including the subtle PIO IN exclusion. Test-matrix
runtimes are accurate to within normal noise. No blocking issues.

One **Important** wording nit (the "margins 8192 vs 128" framing is an
oversimplification of the landing-precision design — the bulk of the first
boot runs at 256, not 8192) and a couple of minor suggestions. None of these
break an operator; all are precision-of-claim improvements.

## What was executed (all PASS / exit 0)

| Command | Result |
|---|---|
| `dh-cli caps` | `kvm_m0_missing_caps=0` |
| `dh-cli cpuid-diff \| tail -1` | masked table hash printed |
| `dh-cli <bad-arg>` | usage printed, exit 2, matches README synopsis exactly |
| `dh-cli skid --samples 1000` | GATE OK, max 31 |
| `dh-cli skid --samples 50000` | GATE OK (3 runs: max 81 outlier, then 33, 31) |
| `dh-cli gate --runs 3` | PHASE-1 GATE: PASS |
| `cargo test -p dh-vmm --test blk_fixture` | 3 passed (0.07s) |
| `cargo test -p nanokernel --test channel_interop` | 1 passed |
| `cargo test -p dh-cli --test boot_hello` | 6 passed (0.16s) |
| `cargo test -p dh-cli --test skid_gate` | 2 passed (0.56s) |
| `cargo test -p determinism-tests --test regression` | 2 passed (4.06s, claim ~4s) |
| `... --test if0_deferral` | 1 passed (31.7s, claim ~32s) |
| `... --test counting_semantics --test counting_smoke` | passed (<1s) |
| `... --test landing_precision` | 2 passed (64.7s, claim ~71s) |
| `... --test timer_determinism` | 1 passed (95.0s, claim ~95s, "100 runs") |
| `... --test m1_acceptance` | 1 passed (0.12s) |
| `bash docs/ops/apply-host-config.sh --verify` | all keys ok (read-only) |
| `cargo run -p dh-worker --bin dh-workerd -- --preflight` | preflight OK, exit 0 |
| `bash ci/check-determinism-class.sh` | 7 keys match, exit 0 |
| `cargo test --workspace` | all green, 0 failed |
| `cargo clippy --workspace --all-targets` | clean |

Tree clean after review (`git status --porcelain` empty).
