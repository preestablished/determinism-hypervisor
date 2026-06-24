# Implementation Plan

## 1. Track The Work

Create or claim a bead before implementation. Use a narrow title such as:

```bash
bd create --title="Generate durable M9 READY snapshot handoff for rom bridge o73" \
  --description="Add an operator path that creates a durable snapstore-backed M9 READY snapshot and writes the private bridge handoff file." \
  --type=task --priority=1
bd update <id> --claim
```

Keep the bead notes sanitized. Record command shapes and pass/fail booleans, not
private values.

## 2. Add The Operator Binary

Add `crates/dh-worker/src/bin/dh-m9-ready-handoff.rs`.

The binary should be x86_64/KVM oriented. On non-x86_64, print a clear error and
exit nonzero instead of trying to compile a partial path.

Suggested CLI:

```text
dh-m9-ready-handoff \
  --private-root <0700 private root> \
  --snapstore-data-root <durable data root> \
  --snapstore-uds <private UDS path> \
  --reference-workload-checkout /home/infra-admin/git/preestablished/reference-workload \
  --workload-manifest /home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/workload-image.yaml \
  --bridge-hypervisor-endpoint unix:///run/dh/grpc.sock \
  --bridge-workload-image-ref <private operator-approved ref> \
  --bridge-capture-spec-ref <private operator-approved ref> \
  --handoff-env <private-root>/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env \
  --public-summary <repo-or-private sanitized summary path> \
  --slot-cores 2-5
```

Also support the documented `DH_M9_*` environment variables:

- `DH_M9_BZIMAGE`
- `DH_M9_INITRAMFS`
- `DH_M9_BASE_IMAGE`
- `DH_M9_GAME_IMAGE`
- `DH_M9_IMAGE_CACHE`

The generator should fail if any required `DH_M9_*` path is missing, unreadable,
or not the expected file/directory type. Do not allow `DH_M9_ALLOW_SKIP=1` to
turn this into a success path.

## 3. Move Test-Only Logic Into A Reusable Module

Do not import `crates/dh-worker/tests/common/mod.rs` from the binary. Instead,
extract or duplicate the production-safe parts into a crate module such as
`crates/dh-worker/src/m9_handoff.rs`.

Useful functions to promote from the tests:

- artifact loading and path validation from `M9LinuxArtifacts`;
- `hash_file`;
- image cache population via content-addressed `image_resolver::cache_key`;
- `m9_linux_machine_config`;
- masked CPUID table validation;
- READY hard cap and memory constants.

Keep test-only behavior out of the module:

- no `TempDir` snapstore root;
- no skip-on-missing-artifact path;
- no fake determinism class unless an explicit test mode uses it.

Update the existing tests only if it reduces duplication without changing their
semantics.

## 4. Build The Durable Snapstore Path

Use a caller-owned `--snapstore-data-root`; never create the actual handoff under
a temp directory. Ensure parent directories are mode `0700`.

The least invasive implementation is to promote `snapstore-server.workspace =
true` from `dh-worker`'s x86_64 dev-dependencies to x86_64 dependencies and use
`snapstore_server::build_server::serve_for_tests` with the durable data root and
private UDS. Despite the helper name, it starts the real server stack and returns
a shutdown handle. The important change from tests is that the data root is
caller-owned and durable.

If the team does not want an operator binary to call `serve_for_tests`, implement
an equivalent in-process server helper in `snapshot-store` first, or make the
generator connect to an already-running snapstore endpoint supplied by
`--snapstore-uds`. In all cases, write the snapstore config needed to serve the
same data root later:

```toml
data_root = "<private snapstore data root>"
grpc_uds_path = "<private snapstore uds path>"
grpc_tcp_addr = "127.0.0.1:7410"
http_addr = "127.0.0.1:7411"
page_channel_path = "<private snapstore page channel path>"
```

## 5. Run The Worker Lifecycle

Create a `WorkerService` configured with:

- `WorkerConfig::from_host_defaults()` as the base;
- real preflight results if the binary runs preflight itself;
- `image_cache_dir = DH_M9_IMAGE_CACHE`;
- `snapstore = Some(Transport::Uds(<private snapstore uds>))`;
- explicit `slot_cores` from CLI or host defaults.

Then perform the same lifecycle as the M9 worker API test:

1. Populate the image cache for bzImage, initramfs, base image, and game image.
2. Build the M9 Linux `MachineConfig` from artifact hashes and masked CPUID.
3. `CreateVm` with a 32-byte deterministic entropy seed.
4. `TakeSnapshot` at icount zero with `seal_input_log = true`.
5. `Run` until `NextSdkEvent` for `detguest_wire::record::EventKind::Ready`.
6. Verify stop reason is `NextSdkEvent` and the SDK event is `Ready`.
7. `TakeSnapshot` at READY with `seal_input_log = true`.
8. Verify READY snapshot has a 32-byte ref, state hash, and matching
   `machine_config_hash`.
9. Privately verify the READY manifest can be read from snapstore.
10. `DestroyVm` for the source lease before restore verification.
11. `RestoreSnapshot` from the READY ref with an empty entropy seed.
12. Verify restored state hash equals the READY state hash.
13. `DestroyVm` for the restored lease.

Use cleanup guards so `DestroyVm` is attempted for any lease created before an
error. The public summary should include only slot counts and booleans.

## 6. Validate Reference-Workload Inputs

Validate that the reference workload checkout and manifest exist:

```text
/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/workload-image.yaml
```

At minimum, record sanitized public metadata from the manifest such as
`schema_version`, `kind`, `meta.name`, and `meta.version`. Do not use this
manifest to derive private bridge refs unless the operator explicitly approves
that policy. The request contract expects the bridge workload image ref and
capture spec ref as handoff fields; treat both as explicit CLI inputs.

If a YAML parser is added, keep it local to `dh-worker` and test parse failures.
If avoiding a new dependency, validate existence and leave ref validation to the
operator-provided strings.

## 7. Write Private And Public Outputs

Create the private output root with `0700`. Write the handoff env file with
`0600` using a temp file in the same directory followed by atomic rename.

Write only these sensitive values to the private env file:

```dotenv
BRIDGE_HYPERVISOR_ENDPOINT=unix:///run/dh/grpc.sock
BRIDGE_PRIVATE_ROOT=<private bridge root>
BRIDGE_WORKLOAD_IMAGE_REF=<operator-approved workload image ref>
BRIDGE_CAPTURE_SPEC_REF=<operator-approved capture spec ref>
BRIDGE_REFERENCE_WORKLOAD_CHECKOUT=/home/infra-admin/git/preestablished/reference-workload
BRIDGE_REAL_SNAPSHOT_REF=<64 hex snapshot ref>
SNAPSTORE_DATA_ROOT=<private snapstore data root>
SNAPSTORE_GRPC_UDS_PATH=<private snapstore uds path>
DH_M9_IMAGE_CACHE=<image cache path>
```

The public summary must not contain any literal value from the private env file.
It may contain:

- M9 artifacts present: yes/no;
- image cache populated: yes/no;
- durable snapstore data root populated: yes/no;
- READY `TakeSnapshot` succeeded: yes/no;
- `RestoreSnapshot` verification succeeded: yes/no;
- source/restored leases destroyed: yes/no;
- private handoff written: yes/no;
- worker slots before/after as counts only.

Add a forbidden-literal sweep before writing the public summary. Compare the
summary against all private input strings and the generated snapshot ref; fail
if any are present.

## 8. Add Operator Docs

Add `docs/ops/rom-bridge-o73-ready-snapshot.md`.

The doc should include:

- expected sibling checkouts;
- `DH_M9_*` artifact setup from `docs/ops/test-partitioning.md`;
- generator command shape with placeholders only;
- how to inspect the private env file mode without printing contents;
- how to start `snapstore-server` against the produced data root;
- how to start `dh-workerd` with `--snapstore-uds` and not `--no-snapstore`;
- how the produced private env file maps to the bridge plan under
  `/home/infra-admin/git/preestablished/rom-operator-bridge/.agents/plans/live-restore-snapshot-acceptance-o73/`.

Do not include real snapshot refs, socket paths under private roots, private
credentials, or raw error excerpts.
