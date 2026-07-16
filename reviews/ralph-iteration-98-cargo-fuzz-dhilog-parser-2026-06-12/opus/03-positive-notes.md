# Positive Notes

### The fuzzed surface is exactly right

`fuzz/fuzz_targets/dhilog_parse.rs` doesn't stop at `parse` returning `Ok` — it walks
*every* post-parse accessor a real consumer touches: `header()`, all seven `Record`
accessors, `body()` on every record, the `canonical()`/`aux()` filtered views, and
`end()`. This matches the cached fuzzing best practice that "decoders over untrusted
bytes must be total" precisely: `parse` is the gate, but the safety claim is that
*everything reachable after the gate* is also infallible, and the target proves that
property rather than just the gate. Adding `end()` in particular is a sharp choice —
it's the one accessor with an explicit `unwrap()` + `unreachable!()` whose safety
depends entirely on `parse`'s END validation holding.

### Standalone workspace isolation is done correctly and is verifiable

The empty `[workspace]` table in `fuzz/Cargo.toml` keeps the libFuzzer binary (which
needs nightly + a sanitizer-instrumented build) out of the main stable build graph.
I confirmed via `cargo metadata --no-deps` that `dh-inputlog-fuzz` does not appear in
the root workspace, and that the root `Cargo.toml` uses explicit `members` with no
globs — so there's no accidental inclusion path. This is the canonical cargo-fuzz
layout, applied correctly.

### Comments explain the *why*, including the operational gotchas

The crate header, the workflow job comments, and the dispatch input descriptions all
explain reasoning rather than restating code: why the fuzz job is hosted (no KVM
needed, must not occupy the single `kvm-intel` box), why `timeout-minutes` is a fixed
1500 (GHA expressions have no arithmetic), and the 6h-hosted-cap-vs-24h-dispatch
interaction. The 24h operator command is written out verbatim in two places
(workflow + `github-runner.md`) so it can't be misremembered. This is exactly the
density a measurement/CI workflow needs.

### Failure routing is wired correctly

`alert-on-failure` gains `dhilog-fuzz` in its `needs` and keeps `if: failure()`, so a
fuzz-found crash on the *scheduled* run correctly trips the existing
visible-issue mechanism. The crash artifact upload (`if: failure()`) and the updated
issue body (which now points the operator at the `dhilog-fuzz-artifacts` upload for
the crashing input) close the loop from "fuzzer found a bug" to "human has the
reproducer." The alert title/body were updated in lockstep rather than left stale.

### Docs were corrected, not just appended

`docs/ops/github-runner.md` didn't bolt on a new bullet and leave the old "not yet
exercised" claim contradicting reality — it split the bullet so cargo-fuzz moves from
the "pre-staged, unproven" bucket to the "actually exercised" bucket, leaving
grpcurl/stress-ng accurately in the former. Keeping that status table honest is the
whole point of the section, and the change respects it.

### `.gitignore` is complete

`target/ corpus/ artifacts/ coverage/ Cargo.lock` — covers everything cargo-fuzz
generates, including the often-forgotten `coverage/` directory and the
standalone-crate `Cargo.lock` (correct to ignore for a `publish = false` fuzz bin).
