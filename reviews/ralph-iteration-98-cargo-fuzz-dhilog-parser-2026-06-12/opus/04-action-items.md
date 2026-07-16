# Action Items

Branch: `ralph/iteration-98-cargo-fuzz-dhilog-parser` — Reviewer: Claude Opus — 2026-06-12

### Critical

- [ ] None. No blocking defects found.

### Important

- [ ] None that block merge. Be aware of two intentional, documented design
      choices (both verified correct as written):
  - `runs-on: ${{ inputs.fuzz_runner || 'ubuntu-latest' }}` resolves to the single
    label `kvm-intel` on dispatch (vs `[self-hosted, kvm-intel]` elsewhere). Correct
    only while exactly one runner advertises `kvm-intel`. See `01-critical-and-important.md`.
  - Dispatching `fuzz_seconds=86400` WITHOUT `fuzz_runner=kvm-intel` lands on a hosted
    runner, gets killed by GitHub's 6h cap, and files a false "nightly-drift FAILED"
    issue. Documented operator footgun, not a code defect.

### Suggestions (optional, non-blocking)

- [ ] Guard or cache the cargo-fuzz install on the hosted path:
      `command -v cargo-fuzz || cargo install cargo-fuzz --locked` (saves ~2–4 min/night).
      (`.github/workflows/nightly-drift.yaml:91`)
- [ ] File a follow-up bead to add a `splice.rs` / `Lineage::new` fuzz target
      (multi-segment composition). Low marginal risk, closes the loop.
- [ ] Add a pre-fuzz assertion that the seed fixtures are non-empty, so a fixture
      rename can't silently empty the corpus:
      `test -s tests/fixtures/v1_minimal.dhilog && test -s tests/fixtures/v1_kitchen_sink.dhilog`.
      (`.github/workflows/nightly-drift.yaml`, before the fuzz step)
- [ ] Consider pinning `dtolnay/rust-toolchain` to a dated nightly to keep
      fuzz-found crashes distinguishable from nightly-toolchain churn.
      (`.github/workflows/nightly-drift.yaml:87`)
- [ ] Add a one-line comment explaining `[profile.release] debug = 1` (line-tables for
      symbolized fuzz backtraces — don't "optimize" it to 0).
      (`crates/dh-inputlog/fuzz/Cargo.toml:30`)

### Verified during review (no action needed)

- [x] `end()` `unwrap()`/`unreachable!()` are panic-safe: `parse` only returns `Ok`
      with `saw_end` set, guaranteeing ≥1 record and an END last. (`reader.rs:310-320`)
- [x] `body()` fixed-offset slicing is dominated by `validate_kind` layout checks —
      no panic on a parsed log. (`reader.rs:157-205` / `516-549`)
- [x] Standalone fuzz crate is NOT in the root workspace
      (`cargo metadata --no-deps` confirms; root `members` are explicit, no globs).
- [x] Seed fixtures exist: `crates/dh-inputlog/tests/fixtures/v1_minimal.dhilog`,
      `v1_kitchen_sink.dhilog`.
- [x] `inputs.x || 'default'` fallbacks resolve to defaults on scheduled (empty-input) runs.
- [x] Corpus seeding order correct: first positional dir (`fuzz/corpus/dhilog_parse`)
      is the writable corpus; `tests/fixtures` is read-only seed input.
- [x] A scheduled fuzz-found crash correctly fails `dhilog-fuzz` → trips
      `alert-on-failure` (`needs` includes it + `if: failure()`), with crash artifact uploaded.
- [x] `reader` module is `pub` in `lib.rs`, so the fuzz target's
      `dh_inputlog::reader::LogReader` path resolves.
