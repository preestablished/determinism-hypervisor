# Critical & Important Findings

## Critical

**None.** No security, data-loss, crash, or broken-build issues. The `dh-proto`
test passes and the full workspace builds clean. No `unsafe` is introduced
(`#![deny(unsafe_code)]` retained, downgraded from `forbid` for the documented
codegen reason — identical to the snapstore-client precedent).

---

## Important

### I-1 (non-blocking): build.rs does not loudly assert the expected package file was produced

- **Severity:** Important (robustness / future-proofing)
- **File:** `crates/dh-proto/build.rs:5-8`
- **Description:** The cached research file (`tonic/prost ... Common Pitfalls`)
  explicitly calls this out:
  > "If compile_protos silently fails to produce the expected package file, the
  > error surfaces later as a confusing 'No such file or directory' at the
  > include! site — fail the build script loudly."
  >
  > "Check the proto `package` matches the `include_proto!` argument exactly."

  Today `compile_protos(...)?` does propagate protoc *invocation* errors (good —
  the `?` satisfies the research's "build.rs propagates errors" check). But the
  failure mode the research warns about is subtler: a successful protoc run that
  emits a file under a *different* package name than the `include_proto!` string
  expects. Here the package is `determinism.hypervisor.v1` (proto line 15) and the
  include is `tonic::include_proto!("determinism.hypervisor.v1")` (lib.rs:19) —
  **they match exactly today**, so this is not a live bug. The risk is purely for
  bead bcb / future edits: if someone renames the package in the `.proto` without
  updating the `include_proto!` string (or vice versa), the build fails at the
  *include* site in lib.rs with an opaque `$OUT_DIR/...rs: No such file` error
  rather than at the build-script boundary where the cause is obvious.
- **Why non-blocking:** The skeleton is correct as written and the test
  (`skeleton_codegen_is_live`) would fail to compile if the include path were
  wrong, so there is end-to-end coverage. This is a "make the next person's
  failure legible" hardening, not a correctness defect. The snapstore-client
  precedent also omits it, so adding it here would actually diverge from the
  precedent this change is otherwise faithfully mirroring — worth a deliberate
  decision rather than a reflexive add.
- **Suggested fix (optional):** add a post-compile existence assertion, or emit
  the package name as a `cargo:rustc-env` so a mismatch is caught at the build
  boundary. Minimal form:

  ```rust
  fn main() -> Result<(), Box<dyn std::error::Error>> {
      std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
      tonic_build::configure()
          .build_server(true)
          .build_client(true)
          .compile_protos(&["../../proto/hypervisor.proto"], &["../../proto"])?;

      // Fail loudly here rather than at the include_proto! site if the package
      // name in the .proto and the include string ever drift apart.
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?)
          .join("determinism.hypervisor.v1.rs");
      assert!(
          out.exists(),
          "codegen did not produce {out:?}; proto `package` and include_proto! \
           string must match (currently determinism.hypervisor.v1)"
      );
      Ok(())
  }
  ```

  If kept symmetric with the sibling, file this as a note on bead bcb instead and
  add it to **both** repos' build scripts at once.
- **Research reference:** `/tmp/ralph60-research.txt` — "tonic/prost Codegen and
  Protobuf Schema Evolution" → Common Pitfalls ("fail the build script loudly")
  and Relevant to Code Review ("Check the proto `package` matches the
  `include_proto!` argument exactly").

---

### Schema-evolution check (PASS — recorded here because it was the headline risk)

The prompt's headline hazard is that skeleton field numbers must match the
normative API.md schema exactly so bead bcb's fill-in is *additive*, not a
*renumber*. I compared character-by-character:

| Message | Skeleton (`proto/hypervisor.proto`) | API.md normative | Match |
|---|---|---|---|
| `SnapshotRef` | `bytes hash = 1;` | `bytes hash = 1;` (§2.1 L72) | ✅ |
| `StateHash` | `bytes hash = 1;` | `bytes hash = 1;` (§2.1 L73) | ✅ |
| `Lease` | `uint64 slot_id = 1; bytes token = 2;` | identical (§2.1 L74) | ✅ |
| `GetWorkerInfoRequest` | `{}` | `{}` (§2.8 L417) | ✅ |
| `GetWorkerInfoResponse` | `worker_id=1, slots_total=2, slots_free=3, class=4, version=5` | identical (§2.8 L418-424) | ✅ |
| `DeterminismClass` | `cpu_model=1, microcode=2, host_kernel=3, vmm_version=4` | identical (§2.8 L425-430) | ✅ |
| `service HypervisorWorker` | `rpc GetWorkerInfo (...)` | same service name; GetWorkerInfo is the §2.8 health leg (API.md L63) | ✅ |

Field types also match (`uint32` for slot counts, `string` for worker_id/version,
message-typed `DeterminismClass class`). The research's schema-evolution rules
("NEVER change or reuse a field number", "verify the subset's field numbers match
the full documented schema so later fill-in is additive") are **satisfied**. The
skeleton correctly omits the §2.1 types it does not yet need (`MachineConfig`,
`BootSpec`, capture types) rather than inventing partial versions — the right call,
since bcb will add them with their normative field numbers untouched.

One naming note (non-blocking, see suggestions): the service is named
`HypervisorWorker`, matching API.md §2 exactly. Good.
