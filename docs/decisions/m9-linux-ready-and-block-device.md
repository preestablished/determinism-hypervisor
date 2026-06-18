# Decision: M9 Linux READY and block device contract

**Bead:** determinism-hypervisor-4s9.5 · **Status:** decided 2026-06-18 ·
**Owner mechanism:** `crates/dh-devices/src/blk.rs` +
`crates/dh-devices/src/detchannel.rs` + Linux guest image contract

## Context

M9 adds a minimal Linux guest path for `bzImage` boot. The upstream planning
tree describes a Linux guest that mounts its read-only game image at `/dev/vdb`
and reaches deterministic readiness through the guest SDK channel. This repo's
implemented device model does not include a deterministic virtio-blk transport;
it already owns a deterministic pv-blk MMIO device, including snapshot/hash
state, at `0xD000_4000`.

Serial console output is useful for diagnosis, but it is not part of the
record/replay contract. Accepting a serial-only readiness marker would make M9
depend on a debug stream instead of the guest SDK event stream that replay and
verification already reason about.

## Decision

M9 uses the existing deterministic pv-blk device at MMIO base `0xD000_4000` for
the Linux game-image path. The Linux guest driver or shim names the read-only
game image `/dev/vdb` from that pv-blk device, preserving the guest-visible
contract without adding a virtio-blk implementation.

A deterministic virtio-blk subset is out of scope for M9 unless a superseding
bead is filed and explicitly replaces this decision. That superseding work must
own the full deterministic device contract, including snapshot sections, state
hash inputs, replay behavior, and any divergence update.

The only accepted Linux READY point is guest-sdk EventKind 14
`Ready{unit, region_count, manifest_generation}` on detchannel after channel
initialization, `Hello`, autostart/control `Start{}`, and expected region
registration are complete. Serial-only markers, console text, or ad hoc MMIO
flags do not satisfy Linux READY for M9 gates.

## Consequences

M9 boot tests and gates must fail if the guest only emits a serial readiness
marker. They must wait for EventKind 14
`Ready{unit, region_count, manifest_generation}` on detchannel.

Linux guest packaging must include a driver or shim that maps the deterministic
pv-blk device at `0xD000_4000` to `/dev/vdb`. Host-side M9 implementation keeps
using `PvBlk` instead of introducing virtio-blk.

The reference-workload documents that mention virtio-blk remain upstream
planning drift until the divergence ledger or upstream tree is amended. This
decision is the local authority for M9 implementation and tests.
