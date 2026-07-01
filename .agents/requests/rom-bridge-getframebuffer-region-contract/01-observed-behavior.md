# Observed Behavior

All observations are from 2026-07-01 against the deployed rom-bridge-o73
runtime on this host: `dh-workerd` (debug build) serving
`unix:///run/dh/grpc.sock`, sessions restored from the READY snapshot produced
by the rom-bridge-o73 handoff (base `QYrqNOdtoptSnwpfTFfVMRVO5gqHTaRMUeSAH9Fv2aE=`
in ListSlots output).

## Failure 1: Immediately After RestoreSnapshot

Bridge session start (`RestoreSnapshot`) succeeds and returns a paused slot.
The first `GetFramebuffer` on that lease fails:

```text
code = FailedPrecondition
message = "GetFramebuffer framebuffer descriptor has zero dimensions"
```

## Failure 2: After Running The Guest

After a `Run { until: FrameBudget(1) }` on the same lease (icount advanced
641,343,512 → 641,530,504, so the guest is executing), `GetFramebuffer` fails
differently:

```text
code = FailedPrecondition
message = "GetFramebuffer unsupported pixel_format 496749568"
```

The "format" value changes with guest execution — it is framebuffer pixel
data being parsed as a descriptor field. The pv-pad frame counter remained 0
in both observations (the guest had not completed a frame), but the failure
mode is independent of that: the region's first 16 bytes are never a
descriptor, so the call can never succeed.

## How To Reproduce Without The Bridge

Any lease on a slot restored from the rom-bridge-o73 READY snapshot will do:

1. `RestoreSnapshot` with the READY snapshot ref → returns lease, slot paused.
2. `GetFramebuffer { lease }` → `FailedPrecondition: descriptor has zero dimensions`.
3. `Run { lease, until: FrameBudget(1) }`, then `GetFramebuffer { lease }` →
   `FailedPrecondition: unsupported pixel_format <garbage>`.
4. `DestroyVm { lease }` to clean up.

The READY snapshot ref is private: it is `BRIDGE_REAL_SNAPSHOT_REF` in the
handoff env file under the operator-private root (this repo's
`docs/ops/rom-bridge-o73-ready-snapshot.md` documents where that lives). The
base hash quoted above is the ListSlots *base image* hash, not the snapshot
ref. You do not need either to reproduce the bug: any guest publishing a D7
`layout_version 1` raw-pixel framebuffer region triggers the same failure —
a locally built `dh-workerd` plus the reference-workload harness
(`refwork-harness` publishes `PublishedRegion::new("framebuffer", FB_BYTES)`)
reproduces it without our deployment, as should a unit test feeding a
headerless 229,376-byte region into the response builder.

`ListSlots` needs no lease and confirms slot state:

```sh
grpcurl -plaintext -proto proto/hypervisor.proto \
  unix:///run/dh/grpc.sock \
  determinism.hypervisor.v1.HypervisorWorker/ListSlots
```

## What Does Work

Everything else on the same lease behaves: `RestoreSnapshot`, `Run`, `Pause`,
`ListSlots`, `DestroyVm`, and `InjectInputs` all succeed against the same
slot. The failure is isolated to the framebuffer read path.
