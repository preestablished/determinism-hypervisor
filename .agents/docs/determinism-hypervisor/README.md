# determinism-hypervisor

A Rust virtual machine monitor (VMM) built directly on **KVM** (x86_64 Intel hosts only)
that provides **bit-deterministic guest execution**: the same snapshot plus the same
input log produces the exact same guest state, instruction for instruction, byte for
byte, every time.

**Read [`../MAP.md`](../MAP.md) first.** This service is item 1 in the platform build
order, co-developed with [`snapshot-store`](../snapshot-store/). Everything else in the
platform — exploration, scoring, replay proof — is built on the guarantee this service
makes. It runs **only on the Intel box** (VT-x/KVM). It does not run on the DGX Spark
(aarch64).

## Purpose

The platform's exploration loop (MAP.md dataflow) forks guest states thousands of times
per minute, runs short bursts of synthesized input against each fork, and keeps the
interesting results. None of that is meaningful unless execution is *exactly*
reproducible: a discovered trajectory is only a result if re-executing its input log
from the root snapshot reproduces it bit-identically. This service is where that
guarantee is manufactured.

It does so by removing or virtualizing every source of nondeterminism visible to the
guest:

| Nondeterminism source | What we do |
|---|---|
| Wall-clock time (TSC, timers) | **Virtual time**: guest-visible time is a pure function of retired guest instructions, never the host clock. Guest reads time through a paravirtual clock device; timer interrupts are injected at exact retired-instruction counts. |
| Interrupt arrival timing | All interrupts are injected from userspace while the vCPU is paused at a **deterministic boundary** (an exact retired-instruction count), located via the PMU retired-instruction counter plus single-step refinement. |
| Entropy (RNG) | A seeded ChaCha20 PRNG behind a paravirtual entropy device. The seed is in the input-log header; every draw is logged. RDRAND/RDSEED are hidden via CPUID. |
| Device I/O ordering and content | All device models are deterministic state machines: read-only block device + copy-on-write overlay, paravirtual controller-input device, deterministic loopback-only network (injected packets land at logged instruction counts), paravirtual event channel. No real hardware passthrough, ever. |
| Host scheduling / SMP races | Single vCPU in v1 (multi-vCPU determinism is documented future work, see ARCHITECTURE.md §11). The vCPU thread is pinned to an isolated host core. |

## Capabilities (normative)

1. **Boot** a minimal guest (minimal Linux or unikernel, supplied by
   `reference-workload`/`guest-sdk`) from a read-only base image with a copy-on-write
   block overlay.
2. **Deterministic run control**: run until {N retired instructions | M virtual
   nanoseconds | N video frames (the frame-boundary exit of the Nth FrameMark — the
   platform's only frame-quantized stop condition; virtual time itself stays a pure
   function of retired instructions) | next guest-SDK event | goal condition on guest
   RAM}, pausing cleanly at a deterministic boundary.
3. **Instruction-precise event landing**: schedule any event (controller input, packet,
   timer, interrupt) to land at retired-instruction count *N*; the run loop arms a
   guest-mode PMU instructions-retired counter with a skid margin, takes the PMI-driven
   exit, then single-steps to exactly *N*.
4. **Snapshot / restore / fork**: full guest state capture (vCPU registers and all
   ancillary state, emulated device state, virtual-time state, dirty pages via KVM
   dirty-log tracking) as *incremental* snapshots relative to a parent; restore and fork
   from any snapshot reference resolved through `snapshot-store`; forks share memory
   copy-on-write.
5. **Input injection**: structured events scheduled at instruction-count or
   virtual-time offsets — for the game demo, a controller button bitmask per video
   frame via a paravirtual pad device; generically, arbitrary device events.
6. **The input log**: the canonical, versioned, byte-stable serialized record of every
   injected event and entropy draw between two snapshots. `snapshot + input log ⇒
   bit-identical re-execution`. This is the platform's most stability-critical schema;
   it is specified byte-by-byte in [API.md §3](API.md).
7. **Guest memory introspection & the capture engine**: read arbitrary guest-physical
   ranges and the framebuffer window while paused, zero-copy on-host (the guest RAM
   mapping is read directly). The **capture engine** (ARCHITECTURE.md §6.10) accepts a
   compiled extraction list (`CaptureSpec`) on `Run`/`TakeSnapshot`, resolves region
   names through the guest-sdk region manifest, and returns packed `feature_bytes` +
   an lz4 framebuffer inline — the orchestrator forwards these to `state-scorer`; the
   scorer never touches workers. The hypervisor is feature-map-agnostic.
8. **Guest event channel**: receive structured events (assertions, reachability,
   coverage beacons, frame markers) from the in-guest agent over guest-sdk's
   **detchannel** (shared-memory rings + PIO detcall doorbell — owned and specified
   by `guest-sdk`; this service implements the host side) without perturbing
   determinism (guest-initiated exits only; every host-side channel mutation is an
   input-log record).
9. **Worker model**: one host runs many **VM slots**; a thin daemon (`dh-workerd`)
   exposes the gRPC API (tonic) and manages slot lifecycle, leases, and health.
10. **Determinism verification mode**: re-execute a `(snapshot, input log)` pair and
    compare a rolling state hash at epoch boundaries; any divergence is a **P0** and is
    automatically bisected to a diverging instruction range with register/page diffs.

## Non-goals

- **Not a general-purpose VMM.** Guests are curated images built by
  `reference-workload`/`guest-sdk` against this VMM's paravirtual device contract. No
  arbitrary OS support, no ACPI, no UEFI, no PCI hotplug, no VGA/BIOS text console
  emulation beyond a debug serial port.
- **No aarch64.** The instruction-counting and interrupt-landing machinery is
  x86_64/VT-x specific. The Spark never runs this service.
- **No real network.** Only the deterministic loopback / injected-packet model. A guest
  can never observe the outside world.
- **No GPU / graphics device.** The "framebuffer" is a plain guest-RAM window the
  emulator writes pixels into; scoring/encoding happen elsewhere.
- **Not a security boundary.** Guests are trusted lab artifacts; KVM's isolation is
  defense-in-depth, not a product feature. No authn on the worker API (trusted lab
  network; `control-plane` fronts external access later).
- **No storage of snapshots.** Pages, manifests, and input logs are persisted by
  `snapshot-store`; this service holds only live slot state and bounded local caches.
- **No search policy, no scoring.** This is the muscle; `exploration-orchestrator` is
  the brain and `state-scorer` is the judge.
- **No multi-vCPU in v1.** See ARCHITECTURE.md §11 for the deterministic-SMP path.
- **No live migration, no nested virtualization, no swapping of guest RAM.**

## The determinism contract (one paragraph, normative)

Given a machine config `C`, a snapshot `S` (resolved via snapshot-store), and an input
log `L` whose header names `S` as its base, executing `L` against `S` under any build of
this service that supports `L.version`, on any supported host (same CPU determinism
class, see ARCHITECTURE.md §7.4), yields a final guest state whose **state hash** equals
`L.end_state_hash`, and whose every intermediate epoch hash matches the recorded epoch
hashes. Any violation is a P0 defect in this service — never the caller's problem.

## Documents

| File | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crate layout, KVM integration, determinism mechanisms in depth (instruction-precise landing, virtual time, entropy, device models), snapshot/fork mechanics, performance engineering, multi-vCPU future work |
| [API.md](API.md) | Complete gRPC surface (tonic/protobuf), the input-log byte-level format spec, the device-blob format, snapshot manifest interchange with snapshot-store |
| [INTEGRATION.md](INTEGRATION.md) | How orchestrator, snapshot-store, state-scorer, replay-renderer, and guest-sdk interact with this service; ASCII sequence diagrams for one exploration step and one replay verification |
| [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md) | Ordered milestones with acceptance criteria, testing strategy (determinism regression in CI), risks and mitigations |

## Glossary

| Term | Definition |
|---|---|
| **icount** | The canonical retired-guest-instruction count since the base snapshot of the current run segment. `u64`. The platform's logical clock. |
| **Deterministic boundary** | A point where the vCPU is paused with the guest at an instruction start, identified by the tuple `(icount, RIP)` (plus `RCX` for REP-prefixed string instructions). All events land, and all snapshots are taken, only at boundaries. |
| **Virtual time / `vns`** | Virtual nanoseconds: `vns = icount * clock_num / clock_den` (default 1 ns per instruction, i.e., a deterministic "1 GHz"). The only time the guest can observe. |
| **Skid margin** | The number of instructions before a target icount at which the PMU overflow interrupt is armed, to absorb hardware PMI skid. Remaining distance is covered by single-stepping. Default 8192. |
| **PMI** | Performance-monitoring interrupt: the host-side overflow interrupt of the guest-mode instructions-retired counter, used to kick the vCPU out of guest mode near (before) a target icount. |
| **Slot** | One VM instance managed by `dh-workerd`: guest RAM mapping, vCPU thread (pinned), device models, run-control state. The unit the orchestrator schedules jobs onto. |
| **Snapshot ref** | BLAKE3-256 of the snapshot manifest's canonical bytes, issued by snapshot-store. The only way services name guest states. |
| **Device blob** | The opaque, versioned byte container (format `DHSNAP`, see API.md §4) holding vCPU registers, emulated device state, virtual-time state, and entropy-PRNG state. Embedded in the snapshot manifest; snapshot-store stores it byte-identically. |
| **Input log** | The canonical record of every injected event and entropy draw between two snapshots (format `DHILOG`, see API.md §3). |
| **Canonical record** | An input-log record that is an *input* to execution (pad state, device event, packet). Required for replay. |
| **AUX record** | An input-log record that is *derived from* execution (entropy draw receipts, timer fires, epoch hashes, SDK-event receipts). Redundant for replay; used for verification and diagnostics. |
| **Epoch / epoch hash** | A periodic verification checkpoint every `epoch_len` instructions (default 50,000,000): a chained BLAKE3 hash over vCPU state + pages dirtied since the previous epoch. |
| **State hash** | The chained epoch hash value at a boundary; two states are bit-identical iff their full hash chains match. |
| **Base image / overlay** | Read-only guest disk image + per-slot copy-on-write block overlay (cluster-mapped). The base image is content-addressed and identical across the fleet. |
| **Pad device** | The paravirtual controller-input device: a latch of button bitmasks that changes only at logged icounts and is read by the guest via MMIO. |
| **Guest channel (detchannel)** | guest-sdk's shared-memory channel: one 2 MiB guest-RAM page (header + region manifest + four SPSC rings) plus PIO detcall registers at `0xD370–0xD39F`. Owned and specified by `guest-sdk`; this service implements the host side (ARCHITECTURE.md §6.6). |
| **Verification mode** | Run mode that re-executes a `(snapshot, log)` pair, recomputes epoch hashes, compares against the log's AUX `EPOCH_HASH` records, and bisects on divergence. |
| **Determinism class** | The tuple (CPU model/family, microcode rev, host kernel version, dh-vmm version). Returned in `TakeSnapshotResponse` and persisted by the orchestrator into lineage-node attrs (the store's manifest carries no metadata — API.md §5.1). Replay across different classes is best-effort, within a class it is guaranteed. |

## Conventions honored (MAP.md)

- Rust 2021+, `tonic` for gRPC. The canonical `hypervisor.proto` is contributed to the
  shared proto set versioned in `control-plane`; until that repo exists, the file lives
  at `proto/hypervisor.proto` in this repo and is the temporary source of truth.
- On-disk / on-wire persisted formats (`DHILOG`, `DHSNAP`) are hand-specified
  fixed-layout little-endian binary with explicit version fields and golden-bytes tests;
  `serde`+`postcard` is used only for internal, non-interchanged debug dumps.
- Exposes `/healthz` and Prometheus `/metrics` over HTTP on port **7401**; gRPC on
  **7400** (TCP) and `/run/dh/grpc.sock` (UDS, on-box fast path). Structured JSON logs
  via `tracing`.
- Determinism bugs are P0: CI runs the determinism regression suite (re-execute,
  compare hashes) on a KVM-capable self-hosted runner on the Intel box — see
  IMPLEMENTATION-PLAN.md.

## Default ports & paths

| Item | Value |
|---|---|
| gRPC (TCP) | `0.0.0.0:7400` |
| gRPC (UDS, on-box) | `/run/dh/grpc.sock` |
| Health + metrics (HTTP) | `0.0.0.0:7401` |
| Slot working dir (overlays, scratch) | `/var/lib/dh/slots/<slot_id>/` |
| Base image cache | `/var/lib/dh/images/` |
| snapshot-store endpoints used | gRPC UDS `/run/snapstore/grpc.sock`, page channel `/run/snapstore/pages.sock` |
