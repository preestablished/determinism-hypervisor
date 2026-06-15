# M6 grpcurl and metrics smoke

This is the operator smoke for the M6 API surface. It is hardware-gated:
run it on the `kvm-intel` host after `dh-workerd` and `snapshot-store`
are serving. `dh-workerd` does not expose gRPC reflection, so every
`grpcurl` invocation below uses the checked-in proto.

## Setup

Start the worker with explicit local endpoints:

```bash
cargo run -p dh-worker --bin dh-workerd -- serve \
  --tcp 127.0.0.1:7400 \
  --http 127.0.0.1:7401 \
  --uds /tmp/dh-grpc.sock \
  --image-cache /var/lib/dh/images \
  --snapstore-tcp http://127.0.0.1:7410
```

Use these shell helpers from the repository root:

```bash
set -euo pipefail

DH_ADDR=127.0.0.1:7400
DH_HTTP=http://127.0.0.1:7401
DH_UDS=/tmp/dh-grpc.sock
SERVICE=determinism.hypervisor.v1.HypervisorWorker
GRPC=(grpcurl -plaintext -import-path proto -proto hypervisor.proto)
GRPC_UDS=(grpcurl -plaintext -unix -import-path proto -proto hypervisor.proto)

b64hex() { printf '%s' "$1" | xxd -r -p | base64 -w0; }
lease_json() {
  jq -cn --arg slot "$SLOT_ID" --arg token "$LEASE_TOKEN_B64" \
    '{slotId:$slot, token:$token}'
}
```

Populate the image cache first, then export base64 versions of the BLAKE3
cache keys. Protobuf JSON encodes `bytes` fields as base64, not hex.

```bash
BASE_IMAGE_HASH_HEX=replace_with_64_hex_base_image_key
KERNEL_HASH_HEX=replace_with_64_hex_landing_loop_elf_key
BASE_IMAGE_HASH_B64=$(b64hex "$BASE_IMAGE_HASH_HEX")
KERNEL_HASH_B64=$(b64hex "$KERNEL_HASH_HEX")
ENTROPY_SEED_B64=$(head -c 32 /dev/zero | base64 -w0)
CMDLINE_B64=$(printf '1000000' | base64 -w0)
```

The same RPCs work over UDS by replacing `"${GRPC[@]}" "$DH_ADDR"` with
`"${GRPC_UDS[@]}" "$DH_UDS"`. A quick UDS reachability check:

```bash
"${GRPC_UDS[@]}" -d '{}' "$DH_UDS" "$SERVICE/GetWorkerInfo"
```

## RPC smoke

List the service methods from the local proto:

```bash
grpcurl -import-path proto -proto hypervisor.proto list "$SERVICE"
```

Worker and slot-read RPCs:

```bash
"${GRPC[@]}" -d '{}' "$DH_ADDR" "$SERVICE/GetWorkerInfo"
"${GRPC[@]}" -d '{}' "$DH_ADDR" "$SERVICE/ListSlots"
```

Start `WatchSlots` in a second shell before a create/destroy operation. It
streams on state transitions; without a transition the deadline is expected.

```bash
"${GRPC[@]}" -max-time 5 -d '{}' "$DH_ADDR" "$SERVICE/WatchSlots" || true
```

Create a VM, capture the lease, and snapshot the starting boundary. This base
snapshot is the `VerifyReplay.base` for the run segment below.

```bash
CREATE_JSON=$(jq -cn \
  --arg base "$BASE_IMAGE_HASH_B64" \
  --arg kernel "$KERNEL_HASH_B64" \
  --arg cmdline "$CMDLINE_B64" \
  --arg seed "$ENTROPY_SEED_B64" \
  '{
    config: {
      version: 1,
      memBytes: "2097152",
      vcpus: 1,
      clockNum: 1,
      clockDen: 1,
      baseImageHash: $base,
      boot: { elf: { kernelHash: $kernel, cmdline: $cmdline } },
      epochLen: "10000",
      hashEpochs: "EPOCHS_ON",
      skidMargin: 8192,
      deviceSet: [2, 3, 4, 6]
    },
    entropySeed: $seed
  }')
CREATE_OUT=$("${GRPC[@]}" -d "$CREATE_JSON" "$DH_ADDR" "$SERVICE/CreateVm")
SLOT_ID=$(jq -r '.lease.slotId' <<<"$CREATE_OUT")
LEASE_TOKEN_B64=$(jq -r '.lease.token' <<<"$CREATE_OUT")
LEASE_JSON=$(lease_json)

BASE_SNAP_OUT=$("${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, sealInputLog:true}')" "$DH_ADDR" "$SERVICE/TakeSnapshot")
BASE_SNAPSHOT_B64=$(jq -r '.snapshot.hash' <<<"$BASE_SNAP_OUT")
```

Execution and input RPCs:

```bash
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, events:[{atIcount:"1000", padSet:{port:0, buttons:1}}]}')" \
  "$DH_ADDR" "$SERVICE/InjectInputs"

"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, icountBudget:"50000", hardIcountCap:"0"}')" \
  "$DH_ADDR" "$SERVICE/Run"

# Method reachability smoke. On the already-paused slot this should return
# FAILED_PRECONDITION; use a concurrent long Run if you need the successful
# pause path.
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" '{lease:$lease}')" \
  "$DH_ADDR" "$SERVICE/Pause" || true
```

Snapshot and introspection RPCs:

```bash
SNAP_OUT=$("${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, sealInputLog:true}')" "$DH_ADDR" "$SERVICE/TakeSnapshot")
SNAPSHOT_B64=$(jq -r '.snapshot.hash' <<<"$SNAP_OUT")
INPUT_LOG_ID_B64=$(jq -r '.inputLogId' <<<"$SNAP_OUT")

"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, ranges:[{gpa:"0", len:"16"}]}')" \
  "$DH_ADDR" "$SERVICE/ReadGuestMemory"

# The landing-loop fixture has no framebuffer descriptor. A framebuffer-capable
# fixture should return pixels; this minimal fixture should reach the method and
# fail with FAILED_PRECONDITION.
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" '{lease:$lease}')" \
  "$DH_ADDR" "$SERVICE/GetFramebuffer" || true

"${GRPC[@]}" -max-time 5 -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, streams:[]}')" \
  "$DH_ADDR" "$SERVICE/StreamGuestEvents"
```

Restore, fork, and verification RPCs:

```bash
"${GRPC[@]}" -d "$(jq -cn --arg snapshot "$BASE_SNAPSHOT_B64" --arg log "$INPUT_LOG_ID_B64" \
  '{base:{hash:$snapshot}, inputLogId:$log, bisectOnDivergence:false}')" \
  "$DH_ADDR" "$SERVICE/VerifyReplay"

RESTORE_OUT=$("${GRPC[@]}" -d "$(jq -cn --arg snapshot "$SNAPSHOT_B64" \
  '{snapshot:{hash:$snapshot}, entropySeed:""}')" \
  "$DH_ADDR" "$SERVICE/RestoreSnapshot")
RESTORED_SLOT_ID=$(jq -r '.lease.slotId' <<<"$RESTORE_OUT")
RESTORED_TOKEN_B64=$(jq -r '.lease.token' <<<"$RESTORE_OUT")
RESTORED_LEASE_JSON=$(jq -cn --arg slot "$RESTORED_SLOT_ID" --arg token "$RESTORED_TOKEN_B64" \
  '{slotId:$slot, token:$token}')

FORK_OUT=$("${GRPC[@]}" -d "$(jq -cn --argjson lease "$RESTORED_LEASE_JSON" \
  '{parent:$lease, count:1}')" "$DH_ADDR" "$SERVICE/Fork")
CHILD_SLOT_ID=$(jq -r '.children[0].slotId' <<<"$FORK_OUT")
CHILD_TOKEN_B64=$(jq -r '.children[0].token' <<<"$FORK_OUT")
CHILD_LEASE_JSON=$(jq -cn --arg slot "$CHILD_SLOT_ID" --arg token "$CHILD_TOKEN_B64" \
  '{slotId:$slot, token:$token}')
```

Phase-later RPCs are still part of the surface and should be invoked in the
smoke. Current expected status is `UNIMPLEMENTED` until their implementation
beads land.

```bash
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, icountBudget:"1000", hardIcountCap:"1000"}')" \
  "$DH_ADDR" "$SERVICE/RunWithFrameCapture" || true

"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" \
  '{lease:$lease, token:"1", mode:"COOP", hardIcountCap:"1000"}')" \
  "$DH_ADDR" "$SERVICE/Quiesce" || true
```

Destroy any live leases:

```bash
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$CHILD_LEASE_JSON" '{lease:$lease}')" \
  "$DH_ADDR" "$SERVICE/DestroyVm"
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$RESTORED_LEASE_JSON" '{lease:$lease}')" \
  "$DH_ADDR" "$SERVICE/DestroyVm"
"${GRPC[@]}" -d "$(jq -cn --argjson lease "$LEASE_JSON" '{lease:$lease}')" \
  "$DH_ADDR" "$SERVICE/DestroyVm"
```

## Metrics audit

Health and metrics are HTTP on port 7401, not gRPC:

```bash
curl -fsS "$DH_HTTP/healthz"
curl -fsS "$DH_HTTP/metrics" | tee /tmp/dh-workerd.metrics
```

ARCHITECTURE §9 requires: per-slot icount rate, exits/sec by reason,
landing single-steps/sec, snapshot ms, fork ms, restore ms, dirty pages per
snapshot, verification failures, and the PMI skid histogram. The emitted
Prometheus family audit is:

| ARCH §9 item | Metric family | Type | Labels |
|---|---|---|---|
| per-slot icount boundary | `dh_worker_slot_icount` | gauge | `slot_id` |
| per-slot icount rate | `dh_worker_slot_icount_rate` | gauge | `slot_id` |
| exits/sec by reason | `dh_worker_exits_total` | counter | `slot_id`, `reason` |
| landing single-steps/sec | `dh_worker_landing_single_steps_total` | counter | none |
| snapshot ms | `dh_worker_snapshot_duration_milliseconds` | histogram | `le` on `_bucket` |
| fork ms | `dh_worker_fork_duration_milliseconds` | histogram | `le` on `_bucket` |
| restore ms | `dh_worker_restore_duration_milliseconds` | histogram | `le` on `_bucket` |
| dirty pages per snapshot | `dh_worker_snapshot_dirty_pages` | histogram | `le` on `_bucket` |
| verification failures | `dh_worker_verification_failures_total` | counter | none |
| PMI skid histogram | `dh_pmi_skid_instructions` | histogram | `le` on `_bucket` |

`dh_worker_exits_total.reason` is one of:

```text
debug dirty_ring_full fail_entry hlt internal_error io_in io_out
irq_window_open mmio_read mmio_write shutdown system_event unknown
x86_rdmsr x86_wrmsr
```

Mechanical audit:

```bash
for family in \
  dh_worker_slot_icount \
  dh_worker_slot_icount_rate \
  dh_worker_exits_total \
  dh_worker_landing_single_steps_total \
  dh_worker_snapshot_duration_milliseconds \
  dh_worker_fork_duration_milliseconds \
  dh_worker_restore_duration_milliseconds \
  dh_worker_snapshot_dirty_pages \
  dh_worker_verification_failures_total \
  dh_pmi_skid_instructions
do
  grep -q "^# TYPE ${family} " /tmp/dh-workerd.metrics
done

grep -Eq '^dh_worker_slot_icount\{slot_id="[0-9]+"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_slot_icount_rate\{slot_id="[0-9]+"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_exits_total\{slot_id="[0-9]+",reason="(debug|dirty_ring_full|fail_entry|hlt|internal_error|io_in|io_out|irq_window_open|mmio_read|mmio_write|shutdown|system_event|unknown|x86_rdmsr|x86_wrmsr)"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_snapshot_duration_milliseconds_bucket\{le="(\+Inf|[0-9.]+)"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_fork_duration_milliseconds_bucket\{le="(\+Inf|[0-9.]+)"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_restore_duration_milliseconds_bucket\{le="(\+Inf|[0-9.]+)"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_worker_snapshot_dirty_pages_bucket\{le="(\+Inf|[0-9.]+)"\} ' /tmp/dh-workerd.metrics
grep -Eq '^dh_pmi_skid_instructions_bucket\{le="(\+Inf|[0-9.]+)"\} ' /tmp/dh-workerd.metrics
```

The in-repo unit guard for the same family list is:

```bash
cargo test -p dh-worker --lib metrics_endpoint_exposes_arch_s9_families -- --nocapture
```
