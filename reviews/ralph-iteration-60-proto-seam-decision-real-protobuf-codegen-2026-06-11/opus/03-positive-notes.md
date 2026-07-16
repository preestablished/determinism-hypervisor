# Positive Notes — patterns worth preserving

### P-1: Byte-for-byte schema fidelity to the normative source

`proto/hypervisor.proto:29-47` reproduces API.md §2.1 (`SnapshotRef`,
`StateHash`, `Lease`) and §2.8 (`GetWorkerInfoRequest/Response`,
`DeterminismClass`) with identical field numbers, types, and even inline comments.
This is exactly the discipline the schema-evolution research asks for ("verify the
subset's field numbers match the full documented schema so later fill-in is
additive, not a renumber"). Because every number matches, bead bcb is a pure add —
no consumer re-serialization risk. Preserve this habit when bcb lands the rest of
§2.

### P-2: Genuinely single-point re-export seam

`crates/dh-proto/src/lib.rs:18-20` confines all generated code to one module
(`pub mod v1 { tonic::include_proto!(...) }`) whose body — and only whose body —
swaps to a crate re-export if/when control-plane adopts hypervisor codegen. The
doc comment (L13-17) names the exact future request (`adopt-hypervisor-proto-v1`)
and asserts "nothing else in the workspace changes." I verified that claim:
`grep` for external consumers of `dh_proto::v1` / `sample_lease` / the removed
`determinism_proto::hypervisor` placeholder across `crates`, `tools`, `tests`
returns **zero** hits, so the seam really is the only coupling point. This is the
key property the decision was supposed to buy, and the code delivers it.

### P-3: Faithful mirroring of the snapstore-client precedent

The change does not just *claim* to mirror snapshot-store — it does so down to the
comment text. `crates/dh-proto/build.rs` and
`../snapshot-store/crates/snapstore-client/build.rs` are near-identical (same
vendored-protoc `set_var("PROTOC", ...)`, same `build_server(true)`/
`build_client(true)`, same `compile_protos` shape, same "No protoc on dev boxes or
CI runners" comment). The `#![deny(unsafe_code)]` downgrade-from-forbid carries the
**same justifying comment** as snapstore-client's lib.rs ("`forbid` is too strong
while tonic codegen is in the tree..."). Two sibling service repos now have one
contract mechanic — exactly the "no two different mechanics for no benefit"
rationale in the decision doc (proto-seam.md:32-37).

### P-4: Test pins both the wire round-trip and the existence of both service halves

`crates/dh-proto/src/lib.rs:40-71` (`skeleton_codegen_is_live`) does more than a
trivial smoke test: it (a) round-trips a `Lease` through prost encode/decode and
asserts equality, (b) references
`hypervisor_worker_client::HypervisorWorkerClient::<Channel>::new` and a generic
function bounded by `hypervisor_worker_server::HypervisorWorker` to force both the
client and server codegen halves to *type-check* (so `build_client`/`build_server`
can't silently regress), and (c) round-trips the §2.8 `GetWorkerInfoResponse`
including the nested `Option<DeterminismClass>`. This is precisely the "prove the
seam end-to-end" goal of bead v8p, and it would catch a codegen-config regression
that a build-only check would miss.

### P-5: Hermetic, runner-provisioning-free build

`build.rs:4` sets `PROTOC` to the vendored binary path before `tonic_build` runs,
so the build does not depend on any system protoc — confirmed by a clean
`cargo build --workspace`. The decision doc (proto-seam.md:46-49) ties this back to
bead py3's runner-provisioning list and correctly notes protoc can be dropped from
it. Good cross-bead bookkeeping: a hermeticity property and the operational ticket
it closes are recorded together.

### P-6: Decision doc follows the established house format

`docs/decisions/proto-seam.md` matches `docs/decisions/tsc-alignment.md`'s header
block (`**Bead:** … · **Status:** decided <date> · **Owner mechanism:** …`) and
`Context → Decision → Consequences` spine, while adding two appropriately scoped
sections for the seam and the skeleton/full-surface split. The rationale is
enumerated (sibling precedent, iteration locality, no build-machinery export,
runner no-op) rather than asserted — a reader can evaluate each independently.

### P-7: Narrowed cross-repo facade rather than a blanket dep

`crates/dh-proto/Cargo.toml:11` changes `determinism-proto` from
`features = ["hypervisor"]` to `features = ["common"]`, dropping the now-superseded
placeholder feature instead of leaving it dangling. The `common` feature and
`PROTO_VERSION` re-export (lib.rs:24-25) are verified to exist in the sibling
(`../control-plane/.../src/lib.rs:7,10`; Cargo.toml feature `common = []`). The
dependency surface shrank to exactly what is still consumed.
