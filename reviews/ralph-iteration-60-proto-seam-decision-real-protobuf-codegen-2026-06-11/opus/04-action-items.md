# Action Items

All items are non-blocking. The branch is APPROVE as-is; these are optional
hardening and follow-ups.

## Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-proto/build.rs:5-8] (optional, coordinate with bcb) Add a
      loud post-`compile_protos` guard that the expected
      `$OUT_DIR/determinism.hypervisor.v1.rs` exists, so a future proto `package`
      vs `include_proto!` drift fails at the build-script boundary rather than as
      an opaque "No such file" at the include site (lib.rs:19). Not a live bug —
      the package string and include string match exactly today. If added, add it
      to `../snapshot-store/crates/snapstore-client/build.rs` too, to keep the two
      sibling build scripts identical (otherwise file as a note on bead bcb).
      Research ref: tonic/prost pitfalls "fail the build script loudly".

### Suggestions
- [ ] [crates/dh-proto/src/lib.rs:27-32] Decide whether `pub fn sample_lease()`
      should stay public (add rustdoc framing it as a downstream fixture) or move
      under `#[cfg(test)]` — it is currently public with no doc and used only by
      the in-crate test. Resolve at bcb.
- [ ] [docs/decisions/proto-seam.md:32-37] Add the concrete sibling file paths
      (`../snapshot-store/crates/snapstore-client/build.rs` and `src/lib.rs`) to
      the "Sibling precedent" bullet so the "mirrors the precedent" claim is
      one-click diffable.
- [ ] [proto/hypervisor.proto:25] (optional) Add a comment inside
      `service HypervisorWorker` enumerating the pending rpc names from API.md §2,
      so bead bcb is a pure additive fill-in. (proto3 `reserved` is for message
      field numbers, not service rpc names — this is a comment, not a statement.)
- [ ] [proto/hypervisor.proto:29-47] Be aware the inline field comments duplicate
      API.md prose; when bcb edits API.md §2.1/§2.8, update these comments in the
      same pass to prevent drift. (Keep the comments — the byte-for-byte fidelity
      is a feature.)

### Follow-up beads already filed (no action needed, recorded for traceability)
- bead **bcb** — fill in the full API.md §2 surface (slot lifecycle, execution,
  introspection, verification, watch streams, error model). Must keep all skeleton
  field numbers untouched (additive only).
- bead **0ic** — control-plane `adopt-hypervisor-proto-v1` adoption request
  (backlog; blocked on bcb).
- bead **py3** — drop protoc from the runner-provisioning list now that
  `protoc-bin-vendored` makes the build hermetic.
