# Suggestions (non-blocking)

## S1: Double CI runs on ralph's direct-push workflow

**Where:** `.github/workflows/ci.yaml:2-4` (`on: pull_request` + `push`).

Ralph pushes iteration branches directly and then merges to `main`, so the full matrix (now 2 host runners + 1 self-hosted) runs on every branch push **and** again on the merge-to-`main` push, plus on any PR. The duplicate `push`/`pull_request` triggers are a known double-run pattern (called out in the security research). With the self-hosted Intel box in the mix, that's repeated occupancy of a scarce runner.

**Rationale:** This is intentional in many repos (you want `main` validated independently), so it's a judgment call, not a defect. If runner time matters, scope `push` to `main` and let `pull_request` cover branches:

```yaml
on:
  pull_request:
  push:
    branches: [main]
```

Note this would stop validating ralph's iteration branches on direct push (they'd only be checked via PR). Given ralph's "branch → review → merge" loop, that may be acceptable — but confirm before changing, since it alters the safety net ralph relies on.

## S2: Concurrency group to cancel superseded runs

**Where:** top of `ci.yaml`.

Adding a concurrency group prevents a stack of in-flight runs on the self-hosted box when commits land quickly:

```yaml
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
```

Keep `cancel-in-progress` off for `main` if you want every merge fully validated; `${{ github.ref }}` already separates branches so per-branch cancellation is safe.

## S3: No `rust-toolchain.toml` pin — `clippy -D warnings` is a moving target

**Where:** repo root (absent file) + `ci.yaml:36`.

`dtolnay/rust-toolchain@stable` floats to the newest stable. Because the host lane now gates on `clippy -D warnings`, a future stable that adds a new lint can turn a green PR red with no code change — exactly the kind of surprise that's annoying in an autonomous ralph loop. The two `is_multiple_of` fixes in this very PR are themselves a recent-stable clippy lint (`manual_is_multiple_of`), which illustrates the drift. Consider pinning:

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.93"
components = ["rustfmt", "clippy"]
```

Trade-off: pinning means you opt into toolchain bumps deliberately (good for determinism, which is literally this project's theme) at the cost of not getting new lints for free. Given the project's name, the deterministic-toolchain argument is strong.

## S4: `ubuntu-24.04-arm` runner-label assumption

**Where:** `ci.yaml:17`.

`ubuntu-24.04-arm` is a GitHub-hosted arm64 label that is generally available for public repos, so this should resolve — but it's worth a one-line confirmation that the org has arm64 minutes/availability, because a missing label yields a job stuck "waiting for a runner" rather than a clean failure. (Moot if C1 leads to dropping the arm leg.)
