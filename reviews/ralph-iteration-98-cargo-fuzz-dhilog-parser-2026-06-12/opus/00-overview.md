# Review Overview — DHILOG parser fuzz target + nightly CI lane

- **Branch:** `ralph/iteration-98-cargo-fuzz-dhilog-parser` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Stats:** 5 files, +121 / -10, 1 commit (`77c9df6`)
- **Verdict:** **APPROVE**

## Summary

This change adds a `cargo-fuzz` / libFuzzer target over the DHILOG read path and
wires a nightly CI lane to run it. It is a clean, well-scoped addition that closes
a real coverage gap: `LogReader::parse` is a total decoder over untrusted bytes
(crash artifacts, operator-supplied logs), and until now nothing exercised it with
hostile input.

What the change does:

- **`crates/dh-inputlog/fuzz/Cargo.toml`** — a NEW standalone fuzz crate
  (`libfuzzer-sys 0.4`, path dep on `dh-inputlog`). It carries its own empty
  `[workspace]` table so the nightly-only libFuzzer binary never enters the main
  workspace's stable build graph. Verified: `cargo metadata --no-deps` on the root
  workspace does NOT list `dh-inputlog-fuzz`, and the root `Cargo.toml` uses
  explicit `members` (no globs, no `exclude` needed). Isolation is correct.
- **`fuzz/fuzz_targets/dhilog_parse.rs`** — the target: parse arbitrary bytes; on
  `Ok`, walk `header()`, every record accessor (`kind/rflags/seq/icount/
  boundary_rip/is_aux/body`), the `canonical()`/`aux()` views, and `end()`. This is
  exactly the right surface — every accessor a replayer/verifier touches after a
  successful parse must be total, and this drives all of them.
- **`fuzz/.gitignore`** — corpus/artifacts/target/coverage/Cargo.lock excluded. Correct.
- **`.github/workflows/nightly-drift.yaml`** — a `dhilog-fuzz` job: 1h nightly on a
  hosted runner (fuzzing needs no KVM and must not occupy the single `kvm-intel`
  box), seeded from the golden v1 fixtures, crash artifacts uploaded on failure,
  and wired into `alert-on-failure`'s `needs`. `workflow_dispatch` gains
  `fuzz_seconds`/`fuzz_runner` inputs for the 24h M5-accept operator run on
  `kvm-intel`. The `inputs.x || 'default'` fallbacks resolve correctly on scheduled
  runs (inputs empty → defaults).
- **`docs/ops/github-runner.md`** — the pre-staged-tools note is corrected:
  cargo-fuzz + nightly are now actually exercised; the 24h dispatch is documented
  with the one-KVM-job caveat.

## Correctness highlights I verified against the fuzzed surface

- **`end()` panic-safety** (`reader.rs:310`): `self.records().last().unwrap()` —
  `validate_records` only returns `Ok` when `saw_end` is true, and `saw_end` is set
  exclusively inside the record loop, so a parsed log always has ≥1 record (the END).
  `last()` is therefore always `Some`. The `unreachable!` arm is genuinely
  unreachable because `EndNotLast` rejects any record after END, so the last record
  IS the END, and `validate_kind` already enforced its 40-byte layout. The
  fuzz target adding `end()` to the walk is a good belt-and-suspenders check on
  exactly this reasoning.
- **`body()` slicing** (`reader.rs:157`): every fixed-offset index (e.g. END's
  `p[8..40]`, EPOCH_HASH's `p[8..40]`, TIMER_FIRE's `u64at(12)`) is dominated by the
  corresponding `validate_kind` layout check, so the `try_into().unwrap()`s cannot
  panic on a parsed log. The fuzzer drives `body()` for every record, which is the
  best possible regression guard for this invariant.

## Scope notes (not defects)

- `splice.rs` (`Lineage`) is also a parse-over-untrusted-bytes surface, but it is a
  thin composition layer on top of `LogReader::parse` (the thing being fuzzed) plus
  pure header comparisons — no new byte indexing. It is a reasonable follow-up
  target, not a gap in THIS change. See `02-suggestions.md`.

Verdict rationale: the target is correct, the surface choice is right, the workspace
isolation is verified, the CI semantics are sound, and the fixtures it seeds from
exist. No correctness or safety defects found. Approve.
