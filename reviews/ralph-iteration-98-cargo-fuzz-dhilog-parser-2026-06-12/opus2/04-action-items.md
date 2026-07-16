# Action Items

### Critical
- [ ] None.

### Important
- [ ] None.

### Suggestions (non-blocking; recommend filing beads, do not block merge)
- [ ] **Persist the fuzz corpus across nightlies.** Add `actions/cache@v4` for `repo/crates/dh-inputlog/fuzz/corpus/dhilog_parse`, keyed `dhilog-fuzz-corpus-${{ github.run_id }}` with `restore-keys: dhilog-fuzz-corpus-`. Today the corpus is `.gitignore`d and uncached, so every scheduled run restarts from only the 2 `tests/fixtures` seeds and discovers nothing cumulatively. Cache is branch-scoped + default-branch-only + opaque-input-only, so poisoning risk is acceptable for a public repo. (S1)
- [ ] **Tighten the dispatch/schedule interference comment** near `.github/workflows/nightly-drift.yaml:14–17`. State explicitly that a 24h `fuzz_runner=kvm-intel` dispatch holds concurrency group `kvm-intel-nightly-drift` for the duration, so the 03:17 scheduled drift/canary run queues behind it and runs up to ~24h late — i.e. running the accept dispatch blanks out timely drift detection for a day. (Behavior verified: `cancel-in-progress: false` keeps one pending run; nightly is delayed, not dropped, given the daily cron.) (S3)
- [ ] **Add a second fuzz target for `splice.rs`** (`Lineage::new` / `extend` / `edges`) as a follow-up bead. The splice-specific stitch checks and the `index - 1` / `len() - 1` arithmetic (`splice.rs:85,113–118`) are not currently hostile-input-driven. Out of scope for this commit. (S4)
- [ ] **Document valid `fuzz_runner` values** (`ubuntu-latest`, `kvm-intel`) in the input `description` or `docs/ops/github-runner.md`, noting a typo'd label strands the run as "in progress" until the 25h timeout — and meanwhile holds the concurrency group, blocking the nightly. Documentation-only. (S5)
- [ ] **(Optional) Consider `taiki-e/install-action@cargo-fuzz`** to shave the ~2–4 min `cargo install cargo-fuzz --locked` step, but only if standardizing on that action elsewhere. The current explicit install is fine. (S2)

### Verification log (for the next reviewer)
- [x] Confirmed cargo-fuzz 0.13.2 + nightly toolchain present; built and ran the verbatim CI command (716k runs, 0 crashes, cov ~351).
- [x] Confirmed via `cargo fuzz build -v` + standalone `rustc -O -Cdebug-assertions` repro that integer-overflow checks are ON in the fuzz build — `debug = 1`-only profile is complete, no `overflow-checks = true` needed.
- [x] Confirmed cargo-fuzz auto-creates the corpus dir if missing → a clean checkout's absent `fuzz/corpus/dhilog_parse` is not a CI failure.
- [x] Read `reader.rs` and `splice.rs` in full; every accessor the target touches is total over already-validated bytes (layout checks in `validate_kind` dominate all `body()` slice indices; `end()`'s `unreachable!` arm is genuinely unreachable).
