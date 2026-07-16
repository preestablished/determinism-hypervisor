# Positive Notes (specific)

- **Accessor coverage is exhaustive and intentional.** `dhilog_parse.rs:18–27` walks *every* public accessor on `Record` plus `canonical()`, `aux()`, and `end()` — not just `parse`. This catches the class of bug the reader's "infallible by construction" design is most exposed to: a `parse` that accepts a log whose accessors then panic (e.g. `end()`'s `unreachable!` arm, or a `body()` `try_into().unwrap()` on a wrong-length payload). The target's doc comment (`dhilog_parse.rs:1–4`) names this invariant precisely.

- **The standalone `[workspace]` in `fuzz/Cargo.toml` (line 19) is the correct cargo-fuzz idiom** and keeps the nightly-only libFuzzer bins out of the stable workspace's build graph — the comment at `Cargo.toml:1–5` explains exactly why. This avoids forcing nightly/sanitizer deps on every `cargo build --workspace`.

- **`debug = 1` is the right, complete profile.** Easy to second-guess ("shouldn't this set `overflow-checks`?"), but I verified cargo-fuzz injects `-Cdebug-assertions` itself and overflow-checks follows it — so integer overflow *does* panic in this build. The author correctly trusted the generated template instead of over-specifying.

- **Hosted-by-default is the right call** (`nightly-drift.yaml:80–81`, comment 76–79): fuzzing needs no KVM, so it must not squat on the single `kvm-intel` measurement box. The `inputs.fuzz_runner || 'ubuntu-latest'` fallback keeps the scheduled run hosted while leaving the 24h escape hatch for the operator. Clean separation of "needs the lab box" (drift/canary) from "doesn't" (fuzz).

- **`rss_limit_mb=4096`** directly encodes the "no unbounded allocation" half of the stated invariant — libFuzzer will flag an OOM as a finding, not just a hang. Good that the invariant in the comment is actually enforced by a flag.

- **`alert-on-failure` wiring is correct and complete.** The new job was added to `needs:` (`nightly-drift.yaml:107`) — easy to forget, which would have made a fuzz crash a silent-red nightly, the exact failure mode the alert block exists to prevent. The alert title/body were also updated (`118–119`) to name DHILOG fuzz and to point at the `dhilog-fuzz-artifacts` upload, so a responder gets the crashing input.

- **Crash-artifact upload is gated `if: failure()`** (`nightly-drift.yaml:97–102`) and points at `fuzz/artifacts/` — the reproducer is preserved exactly when it matters and not otherwise. Matches the alert body's promise.

- **The `timeout-minutes: 1500` comment (`82–84`) is honest about platform behavior** — it explicitly notes the 6h hosted cap clamps the value, rather than implying 25h actually runs on hosted. Good defense against a future reader "fixing" the seemingly-too-large number.

- **The docs change is a genuine correctness fix, not churn.** `docs/ops/github-runner.md` previously claimed cargo-fuzz was "pre-staged, not yet exercised"; this commit makes that false, and the diff updates the doc in the same change (`docs/ops/github-runner.md:94–105`) so the doc and reality stay in sync. The split into "grpcurl/stress-ng still pre-staged" vs "cargo-fuzz now exercised" is accurate.

- **Empirically clean:** 716k runs locally with zero crashes and a stable coverage plateau — the reader's totality claims hold up under real fuzzing, not just by inspection.
