# Bead Graph Handoff

## Current State

`bd show determinism-hypervisor-4s9.18` reports the bead as blocked because:

- no `crates/dh-worker/tests/linux_worker_api.rs` target exists.
- `crates/dh-worker/src/service.rs::boot_slot` returns `UNIMPLEMENTED` for `ResolvedBoot::BzImage`.
- additional implementation blocker: worker `RunRequest.until.next_sdk_event` still returns `UNIMPLEMENTED`, so Ready EventKind 14 cannot be observed through the public worker API.

`bd show determinism-hypervisor-4s9.20` shows that bead owns the missing BzImage worker boot seam:

- title: "Route worker CreateVm BzImage boot through the Linux loader".
- acceptance includes BzImage CreateVm success against the Linux fixture.

But `4s9.20` currently depends on `4s9.18`, which makes the graph operationally inverted.

## Recommended Beads Update

Before the next `/ralph` implementation session, choose one of these two paths.

### Preferred Path: Rewire Dependencies

Make `4s9.20` ready first, then make `4s9.18` depend on it.

Exact command shape to verify in the next session:

```bash
bd show determinism-hypervisor-4s9.18
bd show determinism-hypervisor-4s9.20
bd dep remove determinism-hypervisor-4s9.20 determinism-hypervisor-4s9.18
bd dep add determinism-hypervisor-4s9.18 determinism-hypervisor-4s9.20
bd dep cycles
bd show determinism-hypervisor-4s9.18
bd show determinism-hypervisor-4s9.20
```

After rewiring:

```bash
bd ready
```

Expected result: `4s9.20` should become ready if its other dependencies remain closed.

### Fallback Path: Fold 4s9.20 Into 4s9.18

If the team does not want to edit dependencies, claim `4s9.18` and implement the BzImage worker boot seam inside it as an explicit unblock. Then close or supersede `4s9.20` afterward with a note that its code landed under `4s9.18`.

Exact close path:

```bash
bd update determinism-hypervisor-4s9.20 --append-notes "BzImage CreateVm seam landed under determinism-hypervisor-4s9.18."
bd close determinism-hypervisor-4s9.20 --reason "Implemented under determinism-hypervisor-4s9.18"
```

Exact supersede path:

```bash
bd supersede determinism-hypervisor-4s9.20 --with determinism-hypervisor-4s9.18
```

This is less clean because it makes the bead titles inaccurate, but it avoids being stuck behind the current graph.

## Notes For `/ralph`

When `/ralph` runs, the first picked bead should not start by adding virtio-blk or serial readiness. The correct order is:

1. BzImage CreateVm public worker path.
2. Worker `NextSdkEvent` public run path.
3. Bus-device state hash participation in run and replay.
4. Linux worker API acceptance test for pv-blk `/dev/vdb`, Ready EventKind 14, snapshot/hash/replay, and no pre-Ready host input.

If `/ralph` sees no ready bead, inspect this dependency inversion first.
