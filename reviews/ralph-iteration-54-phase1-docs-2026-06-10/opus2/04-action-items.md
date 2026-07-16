# Action items

### Critical

- [ ] **Fix the dead "see CI for the cross-cc env" pointer** in
  `docs/ops/test-partitioning.md` (host-runnable table, aarch64 row). There is
  no cross-cc env in CI or the repo — the arm lane runs natively on
  `ubuntu-24.04-arm` (`.github/workflows/ci.yaml:38`). Replace the note with the
  actual off-arm prerequisite, e.g.: "aarch64 is built/clipped natively in CI;
  off-arm this needs `rustup target add aarch64-unknown-linux-gnu` and an
  aarch64 linker (`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc`)."
  Otherwise the documented clippy command fails on a stock x86 host with no
  in-repo guidance.

### Important

- [ ] **Re-label the TSC numbers** in `README.md` Measured-numbers section.
  "932 ns vs 1107 ns worst-case alignment error" is wrong: `docs/decisions/tsc-alignment.md:22-25`
  measures these as **ns/call** ioctl latency, and the MSR path's real hazard is
  sync-heuristic value quantization, not a ns "error." Suggested:
  "932 ns vs 1107 ns per restore call; the MSR path also risks KVM sync-heuristic
  value quantization (see `docs/decisions/tsc-alignment.md`)." Drop "alignment error."

### Suggestions

- [ ] **Hedge the macOS host-runnable claim** in `docs/ops/test-partitioning.md`.
  The rust-lld sysroot fallback in `tests/nanokernel/build.rs` genuinely supports
  ELF cross-linking from a Mac, so the claim is plausible — but no CI lane
  exercises macOS, so it's unverified. Add a footnote on the nanokernel row:
  "macOS: builds via the rust-lld sysroot fallback (system `ld` is Mach-O-only,
  skipped); expected to work, not exercised in CI."

- [ ] **Add a measurement date** to the README "Measured numbers" heading
  ("measured 2026-06-10"), matching the dated provenance in
  `docs/decisions/tsc-alignment.md` and `ci/determinism-class.lock`. Future-proofs
  against silent staleness after a re-baseline.

- [ ] **(Optional) Tighten the "runs in CI on every kernel/microcode bump"
  phrasing** in the README R2 section — the test runs per-push + nightly; a bump
  is caught by the nightly *drift* tripwire, which forces the re-baseline that
  re-runs the empirics.

- [ ] **(Follow-up, out of this diff — file a bead) Fix the stale comment** in
  `ci/determinism-class.lock` ("the nightly comparator, which does not exist yet")
  — `nightly-drift.yaml` now runs it. Self-contradiction with CONTRIBUTING.md and
  the new runbook.
