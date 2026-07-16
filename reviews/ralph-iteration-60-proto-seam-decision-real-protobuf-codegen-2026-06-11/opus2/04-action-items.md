# Action Items

Verdict: **APPROVE**. Nothing here blocks merge. All items are optional polish or
forward-tracking notes for bead bcb.

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [crates/dh-proto/build.rs:4] Add a one-line comment noting that
  `std::env::set_var("PROTOC", ...)` becomes `unsafe` in Rust edition 2024 (this
  workspace is 2021, so it is sound today because build scripts run
  single-threaded). If added, mirror the same comment into
  `../snapshot-store/crates/snapstore-client/build.rs` to keep the precedent in
  lockstep — or skip in both. Pure future-proofing breadcrumb.

- [ ] [crates/dh-proto/src/lib.rs:27-33] Decide whether `sample_lease()` should stay
  a public symbol. Its only caller is the `skeleton_codegen_is_live` test in the same
  file (verified: `grep -rn sample_lease crates/` → 2 hits, both here). If it is only
  a test fixture, gate it under `#[cfg(test)]` or add a `// test fixture` comment; if
  it is intentionally public for future bcb consumers, leave it and the comment still
  helps. Non-blocking.

- [ ] [docs/decisions/proto-seam.md] Optional house-format parity: surface the
  "determinism-proto's placeholder hypervisor structs must not be extended"
  guard-rail (currently in the Consequences section) up into the Decision section,
  matching the way `tsc-alignment.md` pins its rejected-path guard-rail in the
  Decision. Stylistic only.

- [ ] [proto/hypervisor.proto — for bead bcb, not this PR] When filling in the full
  §2 surface, port the inline rpc semantic comments from API.md §2 (e.g. `Pause`
  "see §2.9", `Quiesce` "Phase 8, optional", `RunWithFrameCapture` "Phase 7") into
  the generated `.proto` so the schema stays self-documenting. Tracking note for the
  bcb author; no action on this PR.
