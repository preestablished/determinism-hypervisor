# Workflow

The workflow matrix correctly gives each target an isolated corpus cache key and artifact name:
- cache path/key include `${{ matrix.target }}`
- artifact name includes `${{ matrix.target }}`
- `fail-fast: false` preserves signal from the other target if one fails

Concern: the `fuzz_seconds` input is now applied once per matrix target (`.github/workflows/nightly-drift.yaml:118`, `.github/workflows/nightly-drift.yaml:150`). The top-of-file operator guidance still describes a 24h accept run as holding the single `kvm-intel` runner for roughly 24h (`.github/workflows/nightly-drift.yaml:12`, `.github/workflows/nightly-drift.yaml:16`, `.github/workflows/nightly-drift.yaml:19`). With two matrix targets, `fuzz_seconds=86400 -f fuzz_runner=kvm-intel` is now effectively two 24h jobs on the single runner, so the workflow can hold the concurrency group and runner for about 48h.

Recommended fix: either update the comments/input description to make the doubled cost explicit, or add a way to select one target for long operator accept runs. If the intended operator contract is still "24h total", split or reduce the per-target duration accordingly.

No cache or artifact collision issue found.
