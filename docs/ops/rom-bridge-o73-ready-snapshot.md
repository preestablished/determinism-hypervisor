# ROM Bridge o73 READY Snapshot Handoff

This runbook generates a durable M9/reference-workload READY snapshot for
`rom-operator-bridge-o73`. It is an operator path, not a public evidence path:
snapshot refs, private roots, socket paths, raw worker errors, and raw
snapstore errors stay in the private handoff and evidence files.

## Inputs

Expected sibling checkouts:

```text
/home/infra-admin/git/preestablished/determinism-hypervisor
/home/infra-admin/git/preestablished/reference-workload
/home/infra-admin/git/preestablished/snapshot-store
/home/infra-admin/git/preestablished/rom-operator-bridge
```

Export the M9 artifacts documented in `docs/ops/test-partitioning.md`:

```bash
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$m9_artifact_root/bzImage"
export DH_M9_INITRAMFS="$m9_artifact_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
mkdir -p "$DH_M9_IMAGE_CACHE"
```

The operator must also provide private bridge values:

- private bridge root;
- bridge workload image ref;
- bridge capture spec ref.

Do not paste real values into public notes or commits.

## Generate

Prepare a private root outside every git checkout:

```bash
set +x
umask 077
private_root="<private root>"
install -d -m 0700 "$private_root"
```

Run worker preflight:

```bash
cd /home/infra-admin/git/preestablished/determinism-hypervisor
cargo run -p dh-worker --bin dh-workerd -- --preflight
```

Generate the durable snapshot and handoff:

```bash
cargo run -p dh-worker --bin dh-m9-ready-handoff --release -- \
  --private-root "$private_root" \
  --snapstore-data-root "$private_root/rom-bridge-o73/snapstore/data" \
  --snapstore-uds "$private_root/rom-bridge-o73/runtime/snapstore.sock" \
  --reference-workload-checkout /home/infra-admin/git/preestablished/reference-workload \
  --workload-manifest /home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/workload-image.yaml \
  --bridge-hypervisor-endpoint unix:///run/dh/grpc.sock \
  --bridge-private-root "<private bridge root>" \
  --bridge-workload-image-ref "<operator-approved workload image ref>" \
  --bridge-capture-spec-ref "<operator-approved capture spec ref>" \
  --handoff-env "$private_root/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env" \
  --snapstore-config "$private_root/rom-bridge-o73/snapstore/config.toml" \
  --public-summary "$private_root/rom-bridge-o73/public-summary.txt" \
  --slot-cores 2-5
```

The command prints only sanitized status. If it fails after parsing
`--private-root`, private details are written under:

```text
<private root>/rom-bridge-o73/evidence/
```

## Private Outputs

The handoff env is:

```text
<private root>/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env
```

It contains:

```dotenv
BRIDGE_HYPERVISOR_ENDPOINT='unix:///run/dh/grpc.sock'
BRIDGE_PRIVATE_ROOT='<private bridge root>'
BRIDGE_WORKLOAD_IMAGE_REF='<operator-approved workload image ref>'
BRIDGE_CAPTURE_SPEC_REF='<operator-approved capture spec ref>'
BRIDGE_REFERENCE_WORKLOAD_CHECKOUT='/home/infra-admin/git/preestablished/reference-workload'
BRIDGE_REAL_SNAPSHOT_REF='<64 hex snapshot ref>'
SNAPSTORE_DATA_ROOT='<private snapstore data root>'
SNAPSTORE_CONFIG_PATH='<private snapstore config path>'
SNAPSTORE_GRPC_UDS_PATH='<private snapstore uds path>'
DH_M9_IMAGE_CACHE='<image cache path>'
```

It deliberately omits `BRIDGE_CREATE_VM_CONFIG_REF`.

Verify modes without printing private contents:

```bash
stat -c '%a' "$private_root/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env"
stat -c '%a' "$private_root/rom-bridge-o73/snapstore/config.toml"
```

Both files must be `600` or stricter. Private directories should be `700`.

## Serve For Bridge Acceptance

Start snapstore over the generated data root using the private config:

```bash
cd /home/infra-admin/git/preestablished/snapshot-store
nohup setsid cargo run -p snapstore-server --bin snapstore-server -- \
  --config "$private_root/rom-bridge-o73/snapstore/config.toml" \
  > "$private_root/rom-bridge-o73/evidence/snapstore-server.private.log" 2>&1 &
echo $! > "$private_root/rom-bridge-o73/runtime/snapstore-server.pid"
```

The generated config uses the private UDS as the stable endpoint. Any TCP/HTTP
listeners are bound to ephemeral loopback ports and are not part of the bridge
handoff.

Privately verify the manifest:

```bash
cd /home/infra-admin/git/preestablished/snapshot-store
cargo run -p snapstore-cli --bin snapstorectl -- \
  --endpoint "uds:$private_root/rom-bridge-o73/runtime/snapstore.sock" \
  dump-manifest "<private 64 hex snapshot ref>" \
  > "$private_root/rom-bridge-o73/evidence/snapstore-dump-manifest.private.txt" \
  2> "$private_root/rom-bridge-o73/evidence/snapstore-dump-manifest.private.err"
```

Start `dh-workerd` with snapstore enabled:

```bash
cd /home/infra-admin/git/preestablished/determinism-hypervisor
set -a
. "$private_root/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env"
set +a
nohup setsid cargo run -p dh-worker --bin dh-workerd -- serve \
  --uds /run/dh/grpc.sock \
  --image-cache "$DH_M9_IMAGE_CACHE" \
  --snapstore-uds "$SNAPSTORE_GRPC_UDS_PATH" \
  > "$private_root/rom-bridge-o73/evidence/dh-workerd.private.log" 2>&1 &
echo $! > "$private_root/rom-bridge-o73/runtime/dh-workerd.pid"
```

Do not use `--no-snapstore` for the bridge run.

## Bridge Hand-Off

Use the generated env as the determinism-hypervisor snapshot handoff supplement
for:

```text
/home/infra-admin/git/preestablished/rom-operator-bridge/.agents/plans/live-restore-snapshot-acceptance-o73/
```

The bridge still needs bridge-owned service secrets such as the operator
credential and session secret from its private setup. Do not add those secrets
to this repository.

## Sanitized Completion Note

Public notes may include only booleans and counts, for example:

```text
M9 artifact validation: pass
image cache registration: pass
durable snapstore root populated: pass
READY TakeSnapshot: pass
RestoreSnapshot verification: pass
source/restored lease cleanup: pass
private handoff file mode verified: pass
private snapstore config mode verified: pass
public summary redaction sweep: pass
```
