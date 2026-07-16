# Critical and Important Issues

**None.**

Every angle the prompt asked me to probe came back clean. Recording the
verification here so the absence of findings is auditable rather than assumed.

## Verified clean (would have been Critical/Important if wrong)

### Field-number fidelity vs normative API.md §2.1/§2.8 — EXACT MATCH

A mismatch here would be the most damaging possible bug: bead bcb would have to
*renumber* fields (a wire break) instead of additively filling in. I diffed every
line of `proto/hypervisor.proto` against `.agents/docs/determinism-hypervisor/API.md`:

| Message | API.md | Skeleton | Match |
|---|---|---|---|
| `SnapshotRef` | `bytes hash = 1` (§2.1 L72) | `bytes hash = 1` | ✓ |
| `StateHash` | `bytes hash = 1` (§2.1 L73) | `bytes hash = 1` | ✓ |
| `Lease` | `uint64 slot_id = 1; bytes token = 2` (§2.1 L74) | identical | ✓ |
| `GetWorkerInfoRequest` | `{}` (§2.8 L417) | `{}` | ✓ |
| `GetWorkerInfoResponse` | worker_id=1, slots_total=2, slots_free=3, class=4 (DeterminismClass), version=5 (§2.8 L418-424) | identical | ✓ |
| `DeterminismClass` | cpu_model=1, microcode=2, host_kernel=3, vmm_version=4 (§2.8 L425-430) | identical | ✓ |
| `service HypervisorWorker` / `GetWorkerInfo` rpc | §2 L37, L63 | identical name + signature | ✓ |

The `package determinism.hypervisor.v1` declaration matches the `include_proto!`
argument exactly (research pitfall: package/include mismatch surfaces as a confusing
"No such file" at the include site — not a risk here).

### Consumer breakage from the v1 type swap — NONE

`dh-vmm` and `dh-worker` both declare `dh-proto.workspace = true` (pre-existing on
`main`), but **no `.rs` file in the workspace imports `dh_proto::` anything**
(`grep -rn dh_proto crates/ --include='*.rs'` is empty). On `main`,
`dh_proto::v1::Lease` resolved to a determinism-proto *placeholder* struct; on this
branch it resolves to the *generated prost* struct. Because nothing outside
dh-proto's own test references those types, the swap broke nothing.
`cargo build --workspace` is clean.

### determinism-proto facade narrowing `["hypervisor"]` → `["common"]` — COMPLETE

`cargo tree -e features` confirms `common` is the ONLY feature of determinism-proto
enabled anywhere in the workspace tree; dh-proto is the sole consumer and
`default-features = false` is set at the workspace root. No transitive path
re-enables `hypervisor` via feature unification. The placeholder `hypervisor`
module is therefore genuinely dead from this repo, exactly as the decision doc
claims. `common = []` and `PROTO_VERSION` both exist in the determinism-proto source.

### build.rs error propagation + rerun-if-changed — CORRECT

`build.rs` returns `Result` and uses `?` on both `protoc_bin_path()` and
`compile_protos()`, so codegen failure fails the build loudly (research-flagged
pitfall avoided). I captured the build-script output: tonic_build *does* emit
`cargo:rerun-if-changed=../../proto/hypervisor.proto` (and the include dir)
automatically — neither the code nor the doc claims otherwise, so no manual
directive is missing.

### aarch64 cross-build — NO target-side C dependency introduced

`protoc-bin-vendored` is a build-HOST dependency only (`[build-dependencies]`).
The runtime closure (`cargo tree -p dh-proto`) shows no `cc`, `bindgen`,
`openssl-sys`, `ring`, `cmake`, or `pkg-config`; tonic 0.12 + prost 0.13 are pure
Rust (only `libc` FFI bindings appear, which compile no C). aarch64 cross-build is
safe.

### `forbid(unsafe_code)` → `deny(unsafe_code)` downgrade — JUSTIFIED, accurate

Identical justification and wording to the snapstore-client precedent
(`../snapshot-store/crates/snapstore-client/src/lib.rs`): include_proto expands to
code the crate doesn't control, so `forbid` is too strong; manual code keeps the
discipline via `deny`. Comment is accurate.
