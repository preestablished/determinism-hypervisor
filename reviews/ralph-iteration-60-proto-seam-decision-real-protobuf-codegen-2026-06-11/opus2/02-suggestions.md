# Suggestions (non-blocking)

## S1. `set_var("PROTOC", ...)` Rust-2024 unsafe footgun — leave a breadcrumb

**File:** `crates/dh-proto/build.rs:4`

```rust
std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
```

This is fine on edition 2021 (workspace is `edition = "2021"`), but in Rust 2024
`std::env::set_var` becomes `unsafe` (it is process-global and not thread-safe).
A future edition bump would turn this line into a hard compile error with no
in-tree hint as to why. The snapstore-client precedent has the same line with the
same exposure, so this is consistent — but a one-line note ("set_var becomes unsafe
in edition 2024; build scripts are single-threaded so it's sound, wrap in
`unsafe {}` at the bump") would save the next maintainer a search. Lowest priority;
purely a future-proofing breadcrumb. If you add it, do so in *both* repos to keep
the precedent in lockstep, or skip it in both.

## S2. `sample_lease` is now effectively test-only — consider scope

**File:** `crates/dh-proto/src/lib.rs:27-33`

`sample_lease()` is a pre-existing `pub fn` carried over from `main`. On `main` it
returned the determinism-proto placeholder `Lease`; it now returns the generated
prost `Lease`. Its only caller in the entire workspace is the new
`skeleton_codegen_is_live` test (`grep -rn sample_lease crates/` → 2 hits, both in
this file). It is harmless as public API, but if it exists only to seed the
round-trip test, `#[cfg(test)]` or a `// test fixture` comment would clarify intent
and avoid it becoming a load-bearing public symbol by accident. Non-blocking — it
may be intentionally public as a downstream fixture for bcb consumers. Worth a
moment's thought, not a required change.

## S3. Decision doc: a one-line "Alternatives rejected" recap would match house depth

**File:** `docs/decisions/proto-seam.md`

The doc is well-structured and follows the `tsc-alignment.md` house format
(Bead/Status/Owner header → Context → Decision → Consequences). It already lists
the two candidate homes in Context and gives strong rationale. `tsc-alignment.md`
additionally pins a crisp "retained ONLY as a benchmarked reference and must not be
wired into restore" guard-rail for the rejected path. The analogous guard-rail here
("determinism-proto's placeholder hypervisor structs should not be extended") is
present in Consequences — good. No change strictly needed; if you want exact parity
with the house format you could surface that guard-rail one level up into the
Decision section, but this is stylistic.

## S4. Skeleton service drops the `Quiesce`/streaming rpc comments — fine, just confirm bcb captures them

**File:** `proto/hypervisor.proto:24-27`

The skeleton service body keeps only `GetWorkerInfo` and explicitly defers the full
method set to bcb via comment — correct for a seam-proof skeleton. The normative §2
service (API.md L37-66) carries inline semantic notes on several rpcs (e.g. `Pause`
"see §2.9", `Quiesce` "Phase 8, optional", `RunWithFrameCapture` "Phase 7"). Those
notes are *not* lost (they live in API.md, the normative source), so this is not a
gap. Just flagging for the bcb author: when filling in the surface, port those
inline rpc comments from API.md so the generated `.proto` stays self-documenting.
Tracking-only; no action on this PR.
