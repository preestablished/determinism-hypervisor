# Fixture Contract

## Goal

Produce and stage an M9 Linux initramfs and companion game/base images that
satisfy the existing M9 contract and unlock the post-READY gates.

The implementation may live in this repo only if this repo owns the fixture
builder. If the fixture builder lives in a sibling reference-workload repo, this
repo still needs a local verification harness and documentation update that
detects whether the staged artifacts satisfy the contract.

## Ownership Boundary

This repo owns:

- host-side validation of staged artifacts;
- deterministic VMM/worker execution, snapshot, hash, replay, and gate tests;
- Beads notes and local documentation for the accepted M9 contract;
- failure diagnostics when staged artifacts do not satisfy the contract.

The fixture builder or sibling reference-workload repo owns:

- the Linux userspace payload;
- `/etc/detguest/boot.toml` contents;
- the guest-side pv-blk `/dev/vdb` shim or driver;
- the post-READY workload loop and its ABI;
- expected-region memory layout and contents;
- generated `bzImage`, `initramfs.cpio`, `base.img`, and `game.img` artifacts.

If the fixture builder is external, the implementation agent must record the
external repo path, issue ID, and artifact release SHA/hash in the relevant bead
note. If the external builder cannot supply `/dev/vdb` or the post-READY ABI,
keep the producer beads blocked and do not replace them with host-only tests.

## Required Boot Manifest

The initramfs must contain `/etc/detguest/boot.toml` with this shape:

```toml
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/path/to/reference-workload-agent-or-harness"
log_mask = 0x1F

[unit.control]
protocol = "refwork-ctl"
proto_version = 1
game_dev = "/dev/vdb"

[[expected_region]]
name = "wram"
layout_version = 1

[[expected_region]]
name = "framebuffer"
layout_version = 1

[[expected_region]]
name = "meta"
layout_version = 1
```

The exact `exec` path can differ if the reference workload packaging uses
another path, but the control contract and region declarations must not be
weakened.

## Required Guest Behavior

After boot:

1. Initialize detchannel and emit Hello.
2. Register expected regions:
   - `wram`, layout version 1
   - `framebuffer`, layout version 1
   - `meta`, layout version 1
3. Open or otherwise validate the selected M9 game image path `/dev/vdb`.
4. Emit EventKind 14 Ready with:
   - `unit`
   - `region_count`
   - `manifest_generation`
5. After READY, continue running a deterministic workload instead of halting.

The post-READY workload must provide all of these surfaces:

- At least 100 exact retired-instruction landing targets after READY before any
  terminal HLT or shutdown.
- At least one stable interrupt-open window after READY so `TimerArm` or
  scheduled injection can deliver a vector through `inject_at_boundary`.
- Repeated pv-pad frame-counter writes that produce `FRAME_MARK` records.
- Deterministic guest-driven IO. Preferred path:
  - read from `/dev/vdb`;
  - write/read an overlay or deterministic scratch area if the guest contract
    allows it;
  - cause records and final state hashes to be part of the worker-recorded
    DHILOG, not a standalone device unit test.
- Stable expected-region memory contents that can be read through worker
  `ReadGuestMemory`.

## Post-READY Workload ABI

The post-READY loop must be explicit enough that tests do not have to discover
coverage heuristically. The fixture builder should document or expose:

- **Ready boundary:** EventKind 14 is emitted only after expected regions and
  `/dev/vdb` control startup are complete.
- **Counting phase:** after READY, execute a deterministic long-running loop
  with no terminal HLT until after the maximum planned landing/timer/frame/IO
  budget. The loop must provide at least 250,000 retired instructions of stable
  runway after READY, or document a larger exact budget for the tests.
- **Interrupt phase:** keep IF enabled for a documented interval after READY,
  with no pending exception or interrupt shadow at the planned delivery points.
- **Frame phase:** write a strictly increasing pv-pad frame counter on a fixed
  cadence, producing at least 10 `FRAME_MARK` records after READY.
- **IO phase:** perform a deterministic `/dev/vdb` read and, if accepted by the
  workload contract, a deterministic overlay/scratch write-read cycle. The IO
  must happen during a worker-recorded segment so replay sees the records.
- **Region phase:** update or keep stable `wram`, `framebuffer`, and `meta`
  regions so `ReadGuestMemory` can prove layout versions and manifest generation
  match the Ready payload.

If this ABI cannot be provided by the guest, stop and update the fixture
contract. Do not write host tests that infer these phases from unrelated exits.

## Determinism Constraints

The fixture must not introduce raw host nondeterminism:

- No kvmclock dependency.
- No PIT/IOAPIC/in-kernel irqchip.
- No TSC-deadline dependency.
- No RDRAND/RDSEED/host entropy dependency.
- No wall-clock sleep or timeout loops that affect guest-visible state.
- No filesystem timestamps or PID-like values in regions that are included in
  replay state unless the guest explicitly zeros or canonicalizes them.

If Linux or userspace needs a source of entropy, use the deterministic
pv-entropy device and make the seed part of the segment/DHILOG contract.

## Artifact Versioning

The implementation agent must record:

- BLAKE3 hash of `bzImage`.
- BLAKE3 hash of `initramfs.cpio`.
- BLAKE3 hash of `base.img`.
- BLAKE3 hash of `game.img`.
- Git SHA or release identifier of the fixture builder, if available.
- Host kernel and microcode for acceptance runs.

Update `docs/ops/test-partitioning.md` only if the staging command changes. Do
not silently redefine the M9 artifact contract.
