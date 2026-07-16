# Positive Notes

## P1. Field-number fidelity is exact — bcb stays additive

`proto/hypervisor.proto:30-48` reproduces the §2.1/§2.8 message shapes
character-for-character against the normative `API.md`. This is the single most
important property for a skeleton: because every field number matches the full
documented contract, bead bcb fills in the rest of §2 *additively* and never has to
renumber a field (which would be a silent wire break per the prost schema-evolution
research). The author clearly diffed against the normative text rather than
re-deriving the shapes from memory.

## P2. The test is a genuine end-to-end seam proof, not a smoke test

`crates/dh-proto/src/lib.rs:39-72` (`skeleton_codegen_is_live`) does more than
assert "it compiles":
- a real prost `encode_to_vec` → `decode` round-trip on `Lease`,
- existence checks for *both* generated service halves
  (`HypervisorWorkerClient` and the `HypervisorWorker` server trait) — proving
  `build_client` and `build_server` both fired,
- a §2.8 `GetWorkerInfoResponse` round-trip exercising the nested
  `DeterminismClass` message.

This pins the exact thing the decision is about (codegen is live and bidirectional),
so a regression in build.rs or the proto would fail loudly.

## P3. Precedent fidelity is precise, not cargo-culted

`build.rs` and the `forbid`→`deny` lib.rs header are line-for-line faithful to
`../snapshot-store/crates/snapstore-client` — same vendored-protoc pattern, same
`build_server(true)/build_client(true)`, same justification comment. The decision
doc explicitly grounds the choice in that sibling precedent and explains *why*
diverging would be net-negative (two contract mechanics for no benefit). This is the
right kind of consistency: the two service repos now share one contract mechanic.

## P4. The re-export seam is real and documented

`lib.rs:18-23` keeps `dh_proto::v1` as a single `include_proto!` module whose body
can be swapped to a re-export of a future control-plane crate with zero other
workspace churn — and the doc (`proto-seam.md`, "The re-export seam") names the
exact future request ("adopt-hypervisor-proto-v1") mirroring snapstore's. This keeps
the cross-repo migration path open without paying for it now.

## P5. Hermetic, runner-friendly build with no provisioning cost

`protoc-bin-vendored` lives in `[build-dependencies]` and runs on the build host
only; `build.rs` sets `PROTOC` to the vendored path so the build is hermetic w.r.t.
any system protoc. Verified: no target-side C dependency enters the closure, so
aarch64 cross-builds and CI lanes need no protoc provisioning — matching the
doc's "runner provisioning is a no-op" claim (and the note it makes against bead py3).

## P6. Cargo.lock churn is minimal and correct

The lock diff adds only dh-proto's four direct deps; the full prost/tonic/
tonic-build/protoc-bin-vendored transitive closure already existed on `main` (pulled
in by the snapstore-client path dep). The shared one-closure-for-the-workspace
property the doc and Cargo.toml comment assert is real — verified the lock entries
are present on both branches. Exactly the "lock diff matches the manifest change,
nothing else churns" property the workspace research calls for.

## P7. determinism-proto narrowing is complete, not cosmetic

The `["hypervisor"]` → `["common"]` change is backed by a full feature-tree check:
`common` is the only determinism-proto feature enabled anywhere, so the placeholder
hypervisor module is genuinely dead from this repo — and the doc's Consequences
section correctly assigns the cross-repo cleanup to control-plane "at adoption time"
rather than pretending this PR can fix it.
