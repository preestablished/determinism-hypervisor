# guest-sdk

Everything that runs **inside** the guest VM of the deterministic-execution platform,
plus the host-side protocol crate the hypervisor links to talk to it.

Read [`../MAP.md`](../MAP.md) first. This repo is item 2 in the platform build order:
it depends only on `determinism-hypervisor` (for the channel transport semantics) and is
depended on by `reference-workload` (links the SDK), `determinism-hypervisor` (links the
host crate), and indirectly `state-scorer` (consumes published memory regions via the
hypervisor).

## Purpose

The exploration loop needs three things from inside the guest that pure VM-level
introspection cannot give it cheaply:

1. **Structured signals** — assertions, reachability events, coverage beacons, and log
   lines emitted by the software under test, delivered host-ward without perturbing
   determinism.
2. **A supervised workload** — something must be PID 1 in the minimal guest image, bring
   up the paravirtual channel, launch the workload (including the boot-manifest
   autostart that anchors the deterministic READY point), relay the quiesce protocol,
   and report its exit. (Pad input does **not** flow through the agent or the channel —
   it has exactly one path, the hypervisor's pv-pad MMIO latch, read by the SDK.)
3. **Stable, named windows into guest memory** — the host (hypervisor `ReadGuestMemory`,
   and through it `state-scorer`'s feature map) must be able to read "the emulated
   console's work RAM" or "the framebuffer" at fixed guest-physical addresses every
   step, with no guest cooperation per read.

## The three deliverables

| Crate / binary | What it is | Where it runs |
|---|---|---|
| `detguest-agent` | Static Rust binary, PID 1 (or first service) in the minimal guest image. Brings up the **detchannel** (shared-memory rings + port-I/O doorbell), spawns and supervises the workload (boot-manifest autostart → drives the harness control protocol `Hello → LoadGame → Start` → the `Ready` event, the platform's deterministic READY point; `boot.toml` format in API.md §7, driving in ARCHITECTURE.md §4.2), relays control commands and the quiesce protocol, frames stdout/stderr into `LogLine` events. | Inside guest, as root |
| `detguest-sdk` | Rust library linked by software under test. Public API: `assert_always`, `expect_reachable`, `coverage_beacon`, `inject_point`, `register_region`, `poll_input` (pv-pad latch read), `frame_mark`, `quiesce_check`. Writes events directly into its own lock-free ring; never blocks nondeterministically. | Inside guest, in the workload process |
| `detguest-host` | Host-side Rust crate linked by `determinism-hypervisor`. Maps the channel through a `GuestMem` trait, parses/drains event rings, pushes commands/control records, answers `inject_point` queries from a fault plan or the input log, and parses the region manifest. No gRPC here — the hypervisor wraps these types into its `StreamGuestEvents` RPC. | Intel box, inside the hypervisor process |

A fourth, internal crate, `detguest-wire`, holds the `no_std`-compatible byte-level wire
format shared by all three (see `ARCHITECTURE.md` for the workspace layout).

## What this repo is NOT (non-goals)

- **Not a virtio device.** The channel is a custom shared-memory ring pair with a
  port-I/O doorbell (rationale in `ARCHITECTURE.md`). No guest kernel driver, no
  interrupt injection, no device model in the hypervisor beyond a PIO handler.
- **Not the input log.** The canonical input-log format is owned by
  `determinism-hypervisor`. This repo defines which channel interactions *must be
  recorded as* input-log entries (every host write into channel memory, every inject
  decision), not how the log is serialized — the hypervisor logs them as DHILOG
  `DEV_EVENT` records, whose payload encodings its API.md defines.
- **Not the pad-input device.** Pad input has exactly one path: the hypervisor's
  pv-pad MMIO latch (its device, its address map — determinism-hypervisor
  ARCHITECTURE.md §6.4). The SDK's `poll_input()` only wraps the latch read; the
  detchannel carries no pad data.
- **Not the scorer or the feature map.** `reference-workload` owns the per-game feature
  map; `state-scorer` consumes it. This repo only defines the region-manifest format the
  feature map resolves against.
- **No gRPC surface.** `StreamGuestEvents` and all protobuf schemas live in
  `determinism-hypervisor` / `control-plane`. `detguest-host` exposes plain Rust types.
- **No multi-vCPU support in v1.** Guests are single-vCPU; the determinism argument for
  in-guest concurrency assumes one vCPU (see determinism rules in `ARCHITECTURE.md`).
- **No guest networking, no dynamic binary instrumentation, no Windows guests.**
  Coverage beacons are source-level calls compiled into the workload.

## Repo layout (target)

```
guest-sdk/
├── Cargo.toml              # workspace
├── crates/
│   ├── detguest-wire/      # wire format: records, manifest, channel header (no_std)
│   ├── detguest-sdk/       # in-guest instrumentation library
│   ├── detguest-agent/     # PID-1 agent binary
│   └── detguest-host/      # host-side protocol crate
├── image/                  # minimal guest image build (kernel config, initramfs)
└── tests/
    ├── golden/             # wire-format golden vectors
    └── vm/                 # in-VM integration tests (need Intel box / KVM)
```

## Documents

| File | Contents |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crate layout, channel design and why, ring memory layout, quiesce protocol, memory publication + GVA→GPA translation, in-guest determinism rules |
| [`API.md`](API.md) | Complete SDK public API (Rust signatures), byte-level event wire format, region-manifest format, agent control-command set, PIO port map, boot-manifest (`boot.toml`) format |
| [`INTEGRATION.md`](INTEGRATION.md) | How the hypervisor, reference workload, scorer, and input log consume this; sequence diagrams |
| [`IMPLEMENTATION-PLAN.md`](IMPLEMENTATION-PLAN.md) | Ordered milestones with acceptance criteria, testing strategy, risks |

## Glossary

| Term | Meaning |
|---|---|
| **detchannel** | The 2 MiB shared-memory communication region (header + manifest + four rings) plus the PIO doorbell ports. The only guest↔host event/command path (pad input rides the hypervisor's pv-pad MMIO latch, not the channel). |
| **detcall** | A guest-initiated synchronous VM exit via port I/O (`OUT`/`IN` on ports `0xD370–0xD39F`). Deterministic because it is caused by guest code at a fixed instruction count. |
| **ring A / W / C / I** | The four SPSC rings: **A**gent→host events, **W**orkload→host events, host→agent **C**ommands, host→workload control records (ring **I** — quiesce relay only; never pad input). |
| **critical event** | An event the producer may never drop (`Hello`, `Ready`, `AssertViolation`, `InjectQuery`, `RegionRegister`, `WorkloadExited`, first-hit `Reachable`, `NameIntern`, `QuiesceReady`, `FrameMark`). On a full ring the producer rings the doorbell and retries. |
| **READY point** | The deterministic bootstrap anchor: the agent's `Ready` event (after channel init, autostart, the agent-driven `Hello → LoadGame → Start` control-protocol leg, and expected-region registration), at an icount that is a pure function of the WorkloadImage. The orchestrator's root snapshot keys on it. See ARCHITECTURE.md §4.1/§4.2. |
| **boot manifest** | `/etc/detguest/boot.toml`, baked into the initramfs: the agent's unit table (exec path + argv per unit), autostart selection, per-unit control-protocol params (`game_dev`, proto version), and the expected-regions list (names + `layout_version`s) that gates `Ready`. Format owned here (API.md §7); reference-workload bakes the demo's file. |
| **FRAME_MARK** | The frame-boundary signal pair: a `FrameMark` event on ring W plus the pv-pad `FRAME_COUNTER` MMIO write, emitted by `frame_mark()` once per emulated frame. The hypervisor's `at_frame`/`next_sdk_event` semantics key off it. |
| **pv-pad latch** | The hypervisor's MMIO controller-input device (its device map, ARCHITECTURE.md §6.4 there; base `0xD000_1000`). The **only** pad-input path; `poll_input()` wraps the once-per-frame latch read. |
| **droppable event** | `Beacon` and `LogLine`. On a full ring they are counted in the header's drop counters and skipped — deterministically. |
| **region** | A named, mlocked, registered span of workload memory whose guest-physical extents are published in the manifest so the host can read it directly. |
| **region manifest** | Fixed-layout table inside the channel page listing every registered region's GPA extents, guarded by a seqlock generation counter. |
| **name id** | A `u32` assigned by a guest-local counter the first time a static name string is used; the string is sent once in a `NameIntern` event. Keeps hot-path events tiny. |
| **quiesce** | Optional cooperative protocol where the host asks the guest to park at a semantically clean boundary before a snapshot. Usually unnecessary — see `ARCHITECTURE.md`. |
| **icount** | The hypervisor's retired-instruction counter for the vCPU; the platform's clock for the input log. The guest never sees it; the host stamps drained events with it. |
| **GVA / GPA** | Guest-virtual / guest-physical address. The manifest stores GPAs; `ReadGuestMemory` takes GPAs. |
| **fault plan** | The schedule of `inject_point` decisions for a burst, produced by `input-synthesizer` during exploration and recorded into the input log; replay answers from the log. |
