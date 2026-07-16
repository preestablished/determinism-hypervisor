# Suggestions (non-blocking)

### 1. Cache the `cargo install cargo-fuzz` step on the hosted runner

`.github/workflows/nightly-drift.yaml:91`

```yaml
- run: cargo install cargo-fuzz --locked
```

On the hosted (`ubuntu-latest`) nightly path this recompiles cargo-fuzz from source
on every run — roughly 2–4 minutes of wall time and CPU every night, for a tool that
rarely changes. Two easy options:

- Wrap with a guard so a runner that already has it (the `kvm-intel` box pre-stages
  cargo-fuzz per `docs/ops/github-runner.md`) doesn't reinstall:
  ```yaml
  - run: command -v cargo-fuzz || cargo install cargo-fuzz --locked
  ```
- Or add `Swatinem/rust-cache@v2` before the install to cache the build.

Low priority — a 1h fuzz job easily absorbs a few minutes of setup — but it's free
savings and makes the hosted run behave like the pre-staged self-hosted run.

### 2. Add `splice.rs` (`Lineage`) as a follow-up fuzz target

`crates/dh-inputlog/src/splice.rs`

`Lineage::new` / `extend` are another decode-over-untrusted-bytes entry point. They
delegate the heavy lifting to `LogReader::parse` (now fuzzed) and otherwise only do
pure header comparisons (`machine_config_hash`, clock ratio, snapshot-id stitching)
plus `Vec` bookkeeping — so the marginal panic risk is low. But a target that feeds
the fuzzer a length-prefixed list of segments and calls `Lineage::new` would close
the loop on the multi-segment composition logic (e.g. the `parsed.len() - 1` /
`index - 1` arithmetic in the stitch loops). Worth a follow-up bead, not this PR.

### 3. Assert the seed corpus is actually being read

`.github/workflows/nightly-drift.yaml:88-90`

The job seeds from `tests/fixtures`, which contains `v1_minimal.dhilog` and
`v1_kitchen_sink.dhilog` (both confirmed present). If a future refactor moves or
renames those fixtures, cargo-fuzz will silently start from an empty corpus (coverage
quietly drops from ~350 back toward ~31, per the bring-up note) without failing.
A cheap guard before the fuzz run:

```yaml
- run: test -s tests/fixtures/v1_minimal.dhilog && test -s tests/fixtures/v1_kitchen_sink.dhilog
  working-directory: repo/crates/dh-inputlog
```

Optional — makes "the seed corpus exists" a hard precondition instead of an implicit one.

### 4. Consider pinning the nightly toolchain for reproducibility

`.github/workflows/nightly-drift.yaml:87` uses `dtolnay/rust-toolchain@nightly`
(floating). A fuzz target on floating nightly can occasionally break on a nightly
regression unrelated to the code under test, which would then page via
`alert-on-failure` as a false "nightly-drift FAILED." Pinning to a dated nightly
(e.g. `nightly-2026-06-01`) and bumping it deliberately would keep fuzz-found crashes
distinguishable from toolchain churn. Trade-off: you'd stop getting free new
sanitizer/lint coverage. Judgment call — flag, not a request.

### 5. `[profile.release] debug = 1` comment

`crates/dh-inputlog/fuzz/Cargo.toml:30`

`debug = 1` (line-tables) is the right choice for symbolized fuzz backtraces. A
one-line comment saying *why* (so a future reader doesn't "optimize" it away to
`debug = 0`) would match the otherwise excellent comment density of this crate.
