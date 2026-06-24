# Tests And Acceptance

## Unit Tests

Add focused tests for the new module and binary parsing:

1. Missing `DH_M9_*` variables report every missing name and exit nonzero.
2. Artifact validation distinguishes regular files from directories.
3. Image cache population uses lowercase BLAKE3 keys and refuses mismatched
   existing cache entries.
4. CLI rejects missing private handoff inputs.
5. Private output creation sets directory mode `0700` and file mode `0600` or
   stricter on Unix.
6. Public summary redaction fails when any private literal appears in the
   summary.
7. Generated `BRIDGE_REAL_SNAPSHOT_REF` formatting requires exactly 64 lowercase
   hex characters.
8. Cleanup guard attempts to destroy every lease that was created before an
   injected failure.

The tests should be host-runnable where possible. Gate KVM-only tests behind
the same x86_64/KVM pattern used in existing `dh-worker` tests.

## Integration Test

Add an ignored KVM/operator test for the durable path, or extend
`crates/dh-worker/tests/linux_worker_api.rs` with a dedicated durable-root case.

The test should:

1. Create a caller-owned temp directory only for the test's private root.
2. Run the generator against that root with real `DH_M9_*` artifacts.
3. Stop the in-process snapstore server.
4. Start a fresh snapstore server over the same data root.
5. Read the generated READY snapshot manifest from the restarted server.
6. Restore the generated snapshot and compare the restored state hash to the
   READY state hash recorded privately by the generator.
7. Assert source and restored leases are destroyed.
8. Assert public summary contains no generated snapshot ref, private root,
   snapstore UDS path, handoff env path, or lease token bytes.

Use an ignored test name that is explicit, for example:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api \
  --release durable_ready_snapshot_handoff_generates_restoreable_ref \
  -- --ignored --nocapture
```

## Operator Acceptance Commands

On the expected KVM host, run:

```bash
cd /home/infra-admin/git/preestablished/determinism-hypervisor
cargo run -p dh-worker --bin dh-workerd -- --preflight
```

Then export the M9 artifact variables from `docs/ops/test-partitioning.md`:

```bash
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$m9_artifact_root/bzImage"
export DH_M9_INITRAMFS="$m9_artifact_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
mkdir -p "$DH_M9_IMAGE_CACHE"
```

Run the generator with placeholders for private values:

```bash
umask 077
cargo run -p dh-worker --bin dh-m9-ready-handoff --release -- \
  --private-root "<private root>" \
  --snapstore-data-root "<private snapstore data root>" \
  --snapstore-uds "<private snapstore uds>" \
  --reference-workload-checkout /home/infra-admin/git/preestablished/reference-workload \
  --workload-manifest /home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/workload-image.yaml \
  --bridge-hypervisor-endpoint unix:///run/dh/grpc.sock \
  --bridge-private-root "<private bridge root>" \
  --bridge-workload-image-ref "<operator-approved workload image ref>" \
  --bridge-capture-spec-ref "<operator-approved capture spec ref>" \
  --handoff-env "<private root>/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env" \
  --public-summary "<sanitized summary path>"
```

Verify the private handoff file mode without printing contents:

```bash
stat -c '%a %n' "<private root>/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env"
```

Verify the produced manifest privately:

```bash
cd /home/infra-admin/git/preestablished/snapshot-store
cargo run -p snapstore-cli --bin snapstorectl -- \
  --endpoint "uds:<private snapstore uds>" \
  dump-manifest "<private 64 hex snapshot ref>"
```

Raw output from `dump-manifest` stays private.

## Required Pass Criteria

The request is fulfilled only when:

1. Worker preflight passes.
2. All `DH_M9_*` paths exist and are the expected file or directory type.
3. Image cache registration succeeds for every artifact.
4. Durable snapstore data root contains the READY snapshot manifest.
5. `CreateVm`, run-to-READY, READY `TakeSnapshot`, `DestroyVm`,
   `RestoreSnapshot`, and restored `DestroyVm` all succeed.
6. Restored state hash equals the READY snapshot state hash.
7. Private handoff file exists with mode `0600` or stricter.
8. Sanitized public summary contains no private paths, refs, endpoints,
   credentials, lease tokens, or raw worker/snapstore errors.
9. The produced snapstore config and handoff fields are sufficient to start
   `dh-workerd --snapstore-uds` for the bridge.

## Quality Gates Before Commit

Run the smallest host-runnable gates first:

```bash
cargo test -p dh-worker --test arch_dependency_rule
cargo test -p dh-worker --bin dh-m9-ready-handoff
cargo test -p dh-worker
```

If code touches shared worker lifecycle behavior, also run:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api \
  --release -- --ignored --nocapture
```

For determinism-sensitive changes, run the relevant full workspace test command
three times before merging, following the repository Ralph process memory.
