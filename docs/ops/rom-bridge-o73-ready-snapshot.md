# M9 READY Snapshot Handoff (ROM bridge o73, corpus capture, orchestrator M6)

This runbook generates a durable M9/reference-workload READY snapshot on the
Intel reference host and serves the `snapstore-server` + `dh-workerd` pair
that three consumers share: `rom-operator-bridge-o73` bridge acceptance,
reference-workload corpus capture (`refwork-verify` / `m6-session-pipeline.sh`),
and the exploration-orchestrator M6 real-substrate bootstrap. It is an operator
path, not a public evidence path: snapshot refs, private roots, socket paths,
raw worker errors, and raw snapstore errors stay in the private handoff and
evidence files. The one-page operator checklist is
`docs/ops/stack-redeploy-checklist.md`.

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
dist_version=0.2.0   # reference-workload bundle meta.version (epoch 0.2.3)
dist_root="$HOME/.cache/dh-m9/dist-$dist_version"
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$dist_root/bzImage"
export DH_M9_INITRAMFS="$dist_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
mkdir -p "$DH_M9_IMAGE_CACHE"
test -f "$dist_root/staging-stamp.txt"
```

`$dist_root` is the versioned staging root produced by the staging step in
`docs/ops/test-partitioning.md` (decompressed `initramfs.cpio` beside
`bzImage`, plus `staging-stamp.txt`). The handoff's image guard reads that
stamp; a root without it fails closed.

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
  --workload-manifest "/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-$dist_version/workload-image.yaml" \
  --corpus-manifest crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt \
  --staging-stamp "$dist_root/staging-stamp.txt" \
  --bridge-hypervisor-endpoint unix:///run/dh/grpc.sock \
  --bridge-private-root "<private bridge root>" \
  --bridge-workload-image-ref "<operator-approved workload image ref>" \
  --bridge-capture-spec-ref "<operator-approved capture spec ref>" \
  --handoff-env "$private_root/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env" \
  --snapstore-config "$private_root/rom-bridge-o73/snapstore/config.toml" \
  --public-summary "$private_root/rom-bridge-o73/public-summary.txt" \
  --slot-cores 2-5
```

The bundle must live under the reference-workload checkout (`dist/` is
gitignored there); copy the built bundle into
`reference-workload/dist/workload-image-<v>/` if it was produced elsewhere.

Image guard (fail-closed, plan epoch-023): after hashing the four `DH_M9_*`
files the handoff compares them, plus the stamp's manifest kernel hash and
`manifest_emu_version`, against `--corpus-manifest` (default: the checked-in
corpus `expected.txt`; `--staging-stamp` defaults to `staging-stamp.txt`
beside `DH_M9_BZIMAGE`). Any difference stops the run before snapstore or KVM
work with `stage=image guard`. `--allow-image-change` is the ONLY sanctioned
bypass and only for the corpus re-baseline run (package 02 of the epoch-023
plan, run immediately before regenerating `expected.txt`); it prints
`image guard: mismatch-allowed` and the differing key names (never hashes) in
the public summary. `--skip-image-guard` exists for operator opt-out and is
not used by any runbook step.

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

Long-lived services MUST run release builds. `cargo run` without `--release`
launches an unoptimized debug binary; a debug `snapstore-server`/`dh-workerd`
pair is a measured multi-x slowdown on the bridge play path.

```bash
cd /home/infra-admin/git/preestablished/snapshot-store
cargo build --release -p snapstore-server --bin snapstore-server
nohup setsid target/release/snapstore-server \
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
cargo build --release -p dh-worker --bin dh-workerd
nohup setsid target/release/dh-workerd serve \
  --uds /run/dh/grpc.sock \
  --image-cache "$DH_M9_IMAGE_CACHE" \
  --snapstore-uds "$SNAPSTORE_GRPC_UDS_PATH" \
  --staging-stamp "$dist_root/staging-stamp.txt" \
  > "$private_root/rom-bridge-o73/evidence/dh-workerd.private.log" 2>&1 &
echo $! > "$private_root/rom-bridge-o73/runtime/dh-workerd.pid"
```

Do not use `--no-snapstore` for the bridge run. Always pass `--staging-stamp`:
at start the worker re-hashes the staged `bzImage`/`initramfs.cpio` against
the stamp and refuses to serve on a mismatch (pre- and post-epoch blobs
coexist in the content-addressed image cache; the stamp check makes the
stale ones inert). A worker started without it serves an empty
`image_identity` and is NOT healthy for the epoch-023 consumers.

Verify the profile after start: the worker startup log line and `GetWorkerInfo`
report the build profile; both must say `release` before acceptance, and
`GetWorkerInfo.image_identity` must equal
`<bundle version>@<blake3 of staging-stamp.txt>` (compute the hash with
`b3sum "$dist_root/staging-stamp.txt"`).

## Restart After Reboot

`/run/dh` is tmpfs and disappears on reboot; the UDS parent must exist before
the worker binds:

```bash
test -d /run/dh || sudo install -d -m 0755 -o "$USER" /run/dh
```

Then re-source the env and start in order: snapstore (the plan-C supervised
instance; never start a second server on the same data root — `serve()`
unlinks and rebinds an existing UDS and wipes `tmp/`, so two writers corrupt
the store), wait for its `/healthz`, then the worker with the serve block
above. Verify `/healthz`, the `build_profile=release` log line, and
`image_identity` before declaring the stack up. Stop order is the reverse:
kill the worker pid file first, wait for lease cleanup (`ListSlots` shows
every slot free), then snapstore.

## Handoff Record

After the stack is verified, write the private handoff record
`$PR/evidence/hv-epoch-023-stack.txt` (key=value, mode 600; `$PR` is the
reference-workload private root): `worker_git_rev`, `worker_build_profile`,
`worker_uds`, `worker_http`, `worker_image_identity`, `snapstore_uds`,
`snapstore_http`, `snapstore_git_rev`, `ready_snapshot_ref`,
`workload_bundle_version`, `workload_manifest_blake3`,
`staged_initramfs_blake3`, `slot_cores`, `slots_total`, `recorded_at`.
Plan A (`tools/m6-session-pipeline.sh capture --endpoint … --snapshot …`) and
plan D (orchestrator `hypervisor_endpoints`, snapstore endpoint, root ref)
read their values from it; public notes reference it by name only.

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
image guard: match
durable snapstore root populated: pass
READY TakeSnapshot: pass
RestoreSnapshot verification: pass
source/restored lease cleanup: pass
private handoff file mode verified: pass
private snapstore config mode verified: pass
public summary redaction sweep: pass
```

Evidence list (sanitized):

- 2026-09-17, epoch 0.2.3 redeploy (`workload-image-0.2.0`, worker
  `3fcf973`): every line above `pass`, `image guard: match`, worker
  `build_profile=release` with a non-empty `image_identity`, READY ref
  pinned, restore smoke `pass`, restart drill `pass`. Private record:
  `$PR/evidence/hv-epoch-023-stack.txt`; plan completion:
  `~/.agents/projects/determinism-hypervisor/plans/epoch-023-rebaseline-and-intel-box-stack-redeploy/COMPLETION.md`.
