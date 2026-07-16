# Action Items

### Critical
None.

### Important

- [ ] **Fix the landing "8192 vs 128" wording in README.md.** The phrasing
  implies all 10,000 targets run at 8192 on one boot vs 128 on the other. The
  actual test (`tests/determinism/tests/landing_precision.rs`,
  `PRODUCTION_PREFIX = 100`) runs boot A as 8192 for the first 100 targets +
  **256 for the remaining 9,900**, and boot B as uniform 128. Reword to reflect
  the prefix/bulk/second-boot schedule, e.g.:
  "10,000 random targets in 100M instructions, zero overshoots, replayed
  bit-identically across distinct margin schedules (production 8192-prefix +
  256 bulk vs uniform 128) — §3.2 margin-independence proven live."
  Self-contained; no code change, README-only.

### Suggestions

- [ ] **Soften the skid "max 39" line** (README.md) to acknowledge stochastic
  spikes under host load (observed max 81 in one of three 50k runs; gate still
  passed at margin/2 = 4096). Frame the headroom, not a tight ceiling. See S1.
- [ ] **Optionally surface the dh-worker arch-dependency-rule test** as its own
  row in `docs/ops/test-partitioning.md` (currently only under the
  `cargo test --workspace` catch-all, alongside
  `crates/dh-devices/tests/detguest_host_smoke.rs`). Matrix is a highlight
  table by design, so this is opt-in. See S2.
- [ ] **Spell out `run`'s optional flags** instead of `...` for parity with the
  other dh-cli synopsis lines (README.md). Cosmetic. See S3.
- [ ] **(Optional) Retime the full 100-run gate** to confirm the matrix's
  "~32s at 100 runs"; not independently verified at the stated count. See S4.

### Verified-good (no action)
- dh-cli synopsis ↔ usage() exact match
- All kvm-gated test runtimes accurate
- R2/§3.1 exiting-set + PIO IN exclusion faithful to ARCH
- TSC 932/1107, 1e9×2 regression, 10k targets — all match source
- Runbook commands (--verify, --preflight, check-determinism-class) all green
- All doc links resolve; tree/clippy clean
