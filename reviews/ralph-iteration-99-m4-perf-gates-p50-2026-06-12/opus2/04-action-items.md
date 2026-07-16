# Action Items — ralph/iteration-99-m4-perf-gates-p50 (Opus 2nd review)

### Critical

- [ ] **Fix the aarch64 bench link error (C1).** `crates/dh-worker/benches/perf_gates.rs` has `#![cfg(target_arch = "x86_64")]` at the crate root + `harness = false` in `crates/dh-worker/Cargo.toml`. On the `ubuntu-24.04-arm` CI lane the crate compiles to an empty body with no `main` → `error[E0601]: main function not found`, failing `cargo clippy --workspace --all-targets -D warnings` and `cargo build --workspace`. Remove the crate-root `#![cfg]`, move the x86 body into a `#[cfg(target_arch = "x86_64")] mod x86 { ... }`, and provide both a `#[cfg(target_arch = "x86_64")] fn main()` that drives it and a `#[cfg(not(target_arch = "x86_64"))] fn main() {}` stub. Verify: `cargo build --benches -p dh-worker --target aarch64-unknown-linux-gnu` links AND `cargo build --benches -p dh-worker` still builds on x86. (Reproduced empirically; this is a hard merge blocker.)

### Important

- [ ] **Correct the snapshot p50 methodology — it measures the deduped path (I1).** In both `tests/perf_gates.rs:146-177` and `benches/perf_gates.rs:113-145`, all 30 samples ship byte-identical 8k-page content; the pagestore dedups globally by BLAKE3 hash (`snapstore-pagestore/src/ingest.rs:265-267`), so samples 2..30 write zero page bytes. The reported p50 is manifest-put + fsync over an empty page-write set, not the cold 8k-page write the gate intends. `assert_eq!(out.pages_shipped, 8192)` does not catch this (it's `pages.len()` pre-dedup). Fix: vary content per sample (mix the sample index into the fill, re-run the fill inside the timed-setup region) so each sample is a genuine cold write; or, if the deduped path is intentional, say so explicitly in the header and rename the prose. At minimum add a methodology line noting content-identity across samples. (Does not flip the current PASS/FAIL — snapshot already fails its gate — so it does not block bead 8ot, but it must be corrected before this is trusted as a cold-cost gate.)

### Suggestions

- [ ] **Note the warm-cache restore regime (S2).** `tests/perf_gates.rs:181-204`: warm page cache after sample 1 is the *correct* target for "tier-B warm restore" (IMPLEMENTATION-PLAN.md:84); sample 1 is a cold outlier that p50 is robust to. Add a one-line comment so the next reader doesn't re-derive it. No behavioral change.
- [ ] **Dedup the shared consts/helpers (S2).** Move `MEM`, `DIRTY_PAGES`, `config_128()`, `boundary()` into `tests/common/mod.rs` (already shared by the bench via `#[path]`) so the two copies of `boundary()` cannot drift and make the test and bench measure different machine state. Leave the criterion-vs-inline setup blocks duplicated.
- [ ] **Strengthen the snapshot guard if I1 is fixed (S3).** Consider asserting against the store's `(pages_new, pages_deduped)` so accidental re-introduction of cross-sample dedup fails loudly. Only if the engine already needs that telemetry; otherwise a comment suffices.
- [ ] **Report spread alongside p50 (S4).** `p50()` already sorts in place, so printing `samples[0]`/`samples[len-1]` (min/max) next to the median in the `eprintln!` lines is free and lets the operator spot a bimodal (cold/warm or fsync-hiccup) distribution. Observability only.

---

**Self-contained verdict:** REQUEST CHANGES. One Critical (aarch64 CI breakage — hard blocker, empirically reproduced) and one Important (snapshot p50 measures a server-deduped path, under-measuring cold cost). The engine-driving logic, drop placement, timed-window boundaries, and gate hygiene (release-only, debug/KVM self-skip, `#[ignore]`d, single-threaded, real store) are all correct. The measured FAILs are honest platform/threshold signals correctly escalated to bead 8ot; bead 9sb staying open blocked on it is right.
