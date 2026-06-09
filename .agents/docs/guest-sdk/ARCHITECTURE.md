# guest-sdk Architecture

## 1. Workspace / crate layout

```
crates/
├── detguest-wire/        # shared by all three sides; #![no_std] + alloc optional
│   ├── src/lib.rs
│   ├── src/header.rs     # ChannelHeader, RingDesc, drop counters, offsets
│   ├── src/record.rs     # RecordHeader, EventKind, CommandKind, encode/decode
│   ├── src/events.rs     # typed payload structs (AssertViolation, Reachable, ...)
│   ├── src/manifest.rs   # RegionManifest, RegionEntry, Extent, seqlock helpers
│   ├── src/ring.rs       # SPSC ring math (producer/consumer halves, wrap/pad rules)
│   └── src/ports.rs      # PIO port numbers + register encodings (detcall ABI)
├── detguest-sdk/         # std; links into the workload
│   ├── src/lib.rs        # public API (see API.md)
│   ├── src/channel.rs    # mmap channel fd, take ring-W producer / ring-I consumer
│   ├── src/intern.rs     # name → name_id table (guest-local counter)
│   ├── src/beacons.rs    # beacon counter array (auto-registered region)
│   ├── src/regions.rs    # mlock + register_region RPC to agent
│   ├── src/inject.rs     # inject_point: event + detcall IN
│   └── src/pio.rs        # iopl(3) + OUT/IN wrappers; pv-pad MMIO latch mapping
├── detguest-agent/       # std; static binary (musl), PID 1
│   ├── src/main.rs       # init: mounts, channel alloc, CHANNEL_INIT detcall
│   ├── src/supervise.rs  # spawn workload, pipes → LogLine, waitpid → WorkloadExited
│   ├── src/commands.rs   # ring-C consumer: Start/Quiesce/Resume/Shutdown/...
│   ├── src/translate.rs  # /proc/<pid>/pagemap GVA→GPA, extent coalescing
│   └── src/manifest.rs   # manifest writer (seqlock discipline)
└── detguest-host/        # std; linked by determinism-hypervisor
    ├── src/lib.rs
    ├── src/guestmem.rs   # trait GuestMem { read/write at GPA }
    ├── src/channel.rs    # attach, validate header, ring views
    ├── src/drain.rs      # drain rings → Vec<HostEvent>; consumer-index write hook
    ├── src/commands.rs   # push commands / workload-ctrl records; write hook for input log
    ├── src/inject.rs     # InjectResponder + FaultPlan trait
    └── src/manifest.rs   # snapshot-consistent manifest reads (seqlock retry)
```

Dependency edges: `sdk`, `agent`, `host` all depend on `wire`. Nothing else. No
dependency on any other platform repo; `determinism-hypervisor` depends on
`detguest-host`, `reference-workload` depends on `detguest-sdk`.

`detguest-wire` is `#![no_std]` (with `alloc` feature for host-side `Vec` returns) so
the same encode/decode code is bit-for-bit identical in guest and host — the golden
tests run against one implementation, not two.

## 2. Channel design: shared-memory rings + PIO doorbell (and why not virtio-serial)

### Decision

The channel is a **single 2 MiB hugetlb page of guest memory** containing a header,
the region manifest, and four single-producer/single-consumer byte rings, plus a small
set of **port-I/O "detcall" registers** (`0xD370–0xD39F`) for synchronous,
guest-initiated exits (doorbell, channel init, inject query, quiesce ack).

### Why not virtio-serial

- **Interrupts are the enemy.** virtio-console requires the device to inject interrupts
  into the guest to signal host→guest data. Deterministic interrupt delivery is the
  hardest problem `determinism-hypervisor` solves; every additional interrupt source
  multiplies that surface. The detchannel needs **zero** host→guest interrupts: the
  guest discovers host writes by polling at points the guest itself chooses
  (deterministic), and those host writes only happen while the vCPU is paused and are
  recorded in the input log.
- **No guest driver stack.** virtio-serial pulls in the guest kernel's virtio core,
  vring negotiation, and a tty layer whose buffering/flush behavior would sit between
  the SDK and the wire. A raw mmapped ring is ~300 lines and fully under our control.
- **No device model.** The hypervisor side of virtio-serial is a virtqueue state
  machine that must itself be snapshotted deterministically. The detchannel's entire
  host-visible state lives **in guest RAM**, so the ordinary memory snapshot captures
  it for free — producer/consumer indices, undrained events, drop counters, the
  manifest, everything restores bit-identically with the rest of the guest.

### Why PIO and not VMCALL for the synchronous exits

Both are guest-initiated, instruction-boundary VM exits — equally deterministic. PIO
wins on plumbing: KVM routes `OUT`/`IN` on unhandled ports straight to the VMM as
`KVM_EXIT_IO` with the data inline, whereas `VMCALL` is intercepted by KVM's in-kernel
hypercall dispatch (unknown hypercall numbers return `-KVM_ENOSYS` in guest `RAX`;
`#UD` is raised only for `VMCALL` executed from the wrong CPL/mode) — either way the
exit never reaches the VMM. PIO also works from CPL 3 once the process raises its I/O
privilege level via `iopl(3)`: the detcall ports `0xD370–0xD39F` sit above
`ioperm(2)`'s 0–0x3FF limit, so `ioperm` cannot grant them. `iopl` needs
`CAP_SYS_RAWIO`/root, which both agent and workload already have in the minimal image;
the broader all-ports grant is security-irrelevant for trusted lab guests. So the SDK
can detcall directly from the workload without bouncing through the agent.

### The four rings

| Ring | Direction | Producer | Consumer | Size | Carries |
|---|---|---|---|---|---|
| **C** | host → guest | hypervisor | agent | 16 KiB | control commands (Start, Quiesce, Resume, Shutdown, …) |
| **I** | host → guest | hypervisor (and agent, quiesce relay — §6) | SDK (workload) | 16 KiB | workload-directed control records (quiesce relay: QuiesceReq/Resume; reserved for future workload-directed fault/command records). **Never pad input** — see "Pad input is not on the channel" below |
| **A** | guest → host | agent | hypervisor | 64 KiB | agent events (Hello, Ready, WorkloadStarted/Exited, LogLine, QuiesceReady) |
| **W** | guest → host | SDK (workload) | hypervisor | ~1.87 MiB | SDK events (AssertViolation, Reachable, Beacon, InjectQuery, RegionRegister, NameIntern, LogLine, FrameMark) |

Each ring is strictly SPSC. The workload gets the channel fd from the agent at spawn
(inherited memfd-like fd over the hugetlbfs file + `DETGUEST_CHANNEL_FD` env var), maps
it, and takes exclusive ownership of the **W** producer half and the **I** consumer
half. The agent owns **A**-producer and **C**-consumer. The host owns the other halves.

### Pad input is not on the channel (normative)

Controller/pad input has **exactly one** delivery path on the platform: the
hypervisor's **pv-pad MMIO latch**, read once per emulated frame by the SDK. The latch
device — base GPA `0xD000_1000`, registers `PAD0..PAD3`, `FRAME_COUNTER` — is defined
in the hypervisor's MMIO device map (`determinism-hypervisor/ARCHITECTURE.md` §6.4);
this repo cites that address, it does not own it. The SDK's `poll_input()` (API.md
§1.6) is a thin wrapper over that latch read. The latch changes only when a canonical
`PAD_SET` input-log record lands at its icount (hypervisor contract), so each per-frame
read is an MMIO exit at a deterministic icount returning a value that changed only at
logged icounts — fully deterministic, with no channel involvement. Ring I carries **no
pad data**; it is the host→workload control ring only (table above).

**Frame boundary (one signal, two views).** Once per emulated frame the workload calls
`frame_mark()` (API.md §1.6): it writes a critical `FrameMark{frame_index}` record to
ring W, release-stores the producer index, then MMIO-writes the incremented frame index
to pv-pad `FRAME_COUNTER`. The `FRAME_COUNTER` write is the frame-boundary VM exit —
the host records `frame → icount` there (and may drain ring W inside the same exit; the
record is guaranteed visible because it precedes the write, same discipline as
`InjectQuery` before `OUT 0xD384`). The hypervisor's `at_frame` scheduling and
`next_sdk_event` stop conditions key off exactly this pair (its API.md). `FrameMark` is
a first-class event kind; there is no separate "frame-end beacon" convention.

### Channel memory layout (offsets within the 2 MiB page)

All multi-byte fields little-endian. Indices are free-running `u32`, masked by
`size - 1` (sizes are powers of two). Each index lives alone in a 64-byte cache line.

```
0x000000  ChannelHeader
          0x000  magic            u64   = 0x5453_4555_4754_4544  ("DETGUEST" LE)
          0x008  proto_version    u32   = 1
          0x00C  header_flags     u32   bit0: agent_ready, bit1: workload_attached
          0x010  ring_desc[4]           {offset: u32, size: u32} for C, I, A, W
          0x030  reserved         16 bytes
          0x040  drop counters (all u64, written by their ring's producer only):
                 0x040 ringA_dropped_records   0x048 ringA_dropped_bytes
                 0x050 ringW_dropped_records   0x058 ringW_dropped_bytes
                 0x060 ringW_dropped_by_kind[16]  (index = EventKind, kinds 0..15)
0x000100  ringC_prod u32 (host)      — own cache line
0x000140  ringC_cons u32 (agent)
0x000180  ringI_prod u32 (host)
0x0001C0  ringI_cons u32 (SDK)
0x000200  ringA_prod u32 (agent)
0x000240  ringA_cons u32 (host)
0x000280  ringW_prod u32 (SDK)
0x0002C0  ringW_cons u32 (host)
0x001000  Region manifest (28 KiB, format in API.md §4)
0x008000  ring C data (16 KiB)
0x00C000  ring I data (16 KiB)
0x010000  ring A data (64 KiB)
0x020000  ring W data (1,966,080 bytes = 0x1E0000)
0x200000  end
```

Memory-ordering discipline (both sides, both directions):

- Producer: write record bytes, then `Release`-store the new producer index.
- Consumer: `Acquire`-load producer index, read records, then `Release`-store the new
  consumer index.
- Records are 8-byte aligned and never wrap: if a record does not fit in the bytes
  remaining before the ring end, the producer writes a `Pad` record (kind 0) covering
  the tail and starts the real record at offset 0. A `Pad` record's `len` covers the
  whole tail.

### The host side of the channel is part of the input log (key invariant)

Guest reads of channel memory affect guest execution, so **every host mutation of
channel memory is an injected input** and must be recorded by the hypervisor in the
input log as `(icount, what)`:

- pushing a command into ring C / a workload-control record into ring I (record:
  ring id + bytes),
- bumping a consumer index after draining ring A / W (record: ring id + new index),
- the answer returned by an `IN` detcall (record: port + value).

On the wire these land as DHILOG `DEV_EVENT` records — the hypervisor's API.md defines
the `DEV_EVENT` payload encodings for ring pushes, consumer-index bumps, and detcall
`IN` answers (this repo defines *which* mutations must be logged, not how the log
serializes them; see the non-goals in README.md).

The hypervisor only touches channel memory **while the vCPU is paused** (either at an
exploration-step pause or inside a detcall exit). On replay it re-applies each mutation
when the vCPU reaches the recorded icount (pausing there via the PMU mechanism) or
re-answers the detcall when the same `IN` exit occurs. This is what makes the whole
channel — including flow control and drops — bit-deterministic. `detguest-host` exposes
a `ChannelWriteSink` hook so the hypervisor can capture every write it performs (see
API.md §6).

### detcall port map (summary; full register spec in API.md §5)

| Port | Dir | Purpose |
|---|---|---|
| `0xD370` | IN | identify: returns `0xD37E0001` (magic + proto version) |
| `0xD374` | OUT | channel init: GPA low dword |
| `0xD378` | OUT | channel init: GPA high dword |
| `0xD37C` | OUT/IN | OUT: commit init (eax = size in 4 KiB pages); IN: init status |
| `0xD380` | OUT | doorbell (eax = ring mask: bit0 = A, bit1 = W) — host drains now |
| `0xD384` | OUT/IN | inject query: OUT eax = inject seq; IN → packed `FaultDecision` |
| `0xD388` | OUT | quiesce ack (eax = low 32 bits of token) — agent-forced path only |

Every detcall is a synchronous VM exit handled by the hypervisor's PIO handler with the
vCPU paused at an exact instruction boundary; the doorbell is the only mechanism by
which a guest→host ring is drained mid-burst, and it is guest-initiated, hence
replayable.

## 3. Event flow control: never block the workload nondeterministically

Policy, normative:

1. Rings are **fixed size**; producers never wait on the consumer for droppable events.
2. Events are classed **critical** or **droppable** (table in API.md §3.1).
3. **Droppable** (`Beacon`, `LogLine`): if free space < record size, increment
   `dropped_records`, `dropped_bytes`, and the per-kind counter (all in the channel
   header, written by the producer — i.e. guest state, snapshotted and replayable), and
   return without writing. No doorbell, no spin.
4. **Critical** (`AssertViolation`, `InjectQuery`, `RegionRegister`, `NameIntern`,
   first-hit `Reachable`, `WorkloadExited`, `QuiesceReady`, `FrameMark`, `Ready`,
   `Hello`): if the ring is full, ring
   the doorbell (`OUT 0xD380`) and retry. The doorbell exit causes the host to drain
   and bump the consumer index (logged), freeing space. This is a deterministic,
   guest-initiated synchronous wait — the same exits happen at the same icounts on
   replay. Ring W is sized (1.87 MiB) so this is rare.
5. Ring occupancy is itself deterministic because consumption only happens at logged
   points (pause boundaries and doorbell exits). Therefore *which* events get dropped
   is identical on replay. Drop counters are features, not noise: the scorer may read
   them, and replay verification compares them.
6. `coverage_beacon` is engineered to avoid ring traffic entirely on the hot path: it
   increments a slot in the SDK's beacon counter array (an auto-registered published
   region the host reads directly); a `Beacon` ring event is emitted only on the
   **first** hit of each beacon id (discovery signal). Same pattern for `Reachable`:
   first hit emits the event, subsequent hits bump a local counter in the SDK stats
   region.

## 4. Agent lifecycle (PID 1)

```
1. kernel boots (no initrd services) → initramfs /init shim execs
   /sbin/detguest-agent as PID 1 (the image's only init path; no other init
   binary exists)
2. mount /proc, /sys, devtmpfs; mount hugetlbfs at /dev/hugepages
3. allocate channel: open hugetlbfs file, ftruncate 2 MiB, mmap, zero,
   write ChannelHeader + ring descriptors
4. resolve channel GPA: /proc/self/pagemap for the mapped hugepage
5. detcall CHANNEL_INIT (ports 0xD374/0xD378/0xD37C); host validates magic at
   that GPA, maps it, IN 0xD37C must return 0 (OK)
6. set header_flags.agent_ready = 1; emit Hello on ring A; doorbell
7. if the image's boot manifest (/etc/detguest/boot.toml, baked into the
   image; format owned by this repo — API.md §7) configures an autostart unit:
   start it locally (same code path as StartWorkload — NO ring-C command
   involved); if the unit declares a control protocol, drive the agent's leg of
   the harness control protocol through Start{} (§4.2); wait until every region
   named in the boot manifest's expected-regions list is live in the manifest
   at its pinned layout_version, then emit Ready on ring A + doorbell — the
   deterministic READY point (§4.1)
8. poll ring C for commands (poll cadence: every pass through the supervise loop
   and on SIGCHLD; cadence is deterministic — see §7)
9. on StartWorkload: set up stdout/stderr pipes, pass channel fd
   (DETGUEST_CHANNEL_FD), exec the configured workload binary as root — the
   SDK raises iopl(3) itself in init() (baked into the image; the command's
   `unit` field selects among the boot manifest's preconfigured unit entries,
   API.md §7 — argv is NOT sent over the wire, keeping the wire small and the
   image immutable)
10. supervise loop: drain workload pipes → LogLine events on ring A (droppable);
    relay Quiesce commands (see §6); reap on exit → WorkloadExited (critical)
11. on Shutdown: kill workload, emit WorkloadExited if needed, sync, reboot(2)
    with RB_POWER_OFF
```

The agent is a single-threaded `epoll` loop (pipes + signalfd + a timerfd driven by
virtual time). One thread = no scheduler-dependent interleavings inside the agent.

### 4.1 The deterministic READY point (normative contract)

The platform's experiment bootstrap — owned by `exploration-orchestrator` (its
`CreateVm → Run(until READY) → TakeSnapshot → CreateNode(root, node_id=0)` sequence) —
keys on a single, reproducible guest event. This repo defines it:

- **Event kind:** `Ready` (EventKind 14, critical, ring A — payload in API.md §3.2).
  It absorbs every prior "READY beacon" notion (including the hypervisor's old
  pv-evtchn "stream 0 = READY").
- **When it fires:** the agent emits `Ready` + doorbell only after, in order:
  (1) CHANNEL_INIT completed with status 0 and `Hello` was emitted; (2) the boot
  manifest's autostart unit (if configured — API.md §7) was exec'd and, for a unit that
  declares a control protocol, the agent's protocol leg completed **through `Start{}`
  with no `Fault`** (§4.2 — so the workload is already inside its free-running frame
  loop at READY); (3) every region named in the boot manifest's expected-regions list
  is live in the region manifest at its pinned `layout_version` (registered via the
  agent's publication path, §5). With no autostart unit configured, `Ready` fires
  immediately after `Hello` with `region_count = 0`.
- **What it guarantees:** from power-on to the `Ready` doorbell exit, guest execution
  consumes **no host-injected input**: the host MUST NOT push ring-C/ring-I records or
  land `PAD_SET` records before it has drained `Ready`, and the pv-pad latches hold
  their reset value (0). Autostart is agent-local precisely so no host command precedes
  READY, and the §4.2 control-protocol exchange is in-guest socketpair traffic — not
  host input. Therefore the icount at the `Ready` doorbell exit is a pure function of
  the WorkloadImage — **bit-reproducible across boots of the same image**. That icount
  is the deterministic READY point; the root snapshot taken there is identical for
  identical images. READY therefore implies: regions live, workload started (and
  `Start` already issued), zero host input consumed.
- The hypervisor's run controls (`next_sdk_event` / event-kind stop conditions, its
  API.md) are how the orchestrator expresses "Run until READY".

### 4.2 The agent's leg of the harness control protocol (normative)

`reference-workload` owns the **wire protocol** — the `CtlMsg` message set, postcard
framing, transport (`socketpair(AF_UNIX, SOCK_SEQPACKET)`, child end inherited as
fd 3), `proto_version`, and per-message ordering legality are its API.md §3. This
section owns the **driving**: when the agent initiates each message relative to boot,
region registration, and the `Ready` event; reference-workload cites this section for
its harness side.

For a unit whose boot-manifest entry declares a control protocol (API.md §7
`[unit.control]`), the agent, after fork+exec (autostart or ring-C `StartWorkload`):

1. **`Hello{proto_version}` →** (from the boot manifest's pinned value); awaits
   `HelloAck`. Version mismatch ⇒ error path below.
2. **`LoadGame{dev_path}` →** (`dev_path` = the boot manifest's `game_dev`); awaits
   `GameLoaded`.
3. **Region registration (harness-driven):** the agent services the harness's
   `RegisterRegion` messages as they arrive, performing the same pagemap
   translation + manifest publication as the SDK path (§5), then awaits the harness's
   socket-level `Ready{frame: 0}` — sent only after all its registrations
   (reference-workload API.md §3.2). That socket-level `Ready` is the **harness's**
   message; it is distinct from the agent's ring-A `Ready` event and is never visible
   to the host.
4. **`Start{}` →** the harness enters its free-running frame loop. There is no
   StartAck by design (reference-workload §3.2): `Start` *succeeded* iff the send was
   accepted and no `Fault` has arrived by step 5; the running loop's `FrameMark`s are
   the ongoing evidence.
5. **Gate, then ring-A `Ready`:** with (a) `Start` succeeded and (b) every
   expected-region live in the manifest at its pinned `layout_version`, the agent
   emits ring-A `Ready` + doorbell — the deterministic READY point (§4.1). The
   ordering is deliberate: READY implies regions-live **and** workload-running, so the
   root snapshot is taken with the harness already inside the loop, and **every
   restored fork resumes there** — the worker-driver path never pushes `Start` (or any
   other command); the first host action after any restore is landing `PAD_SET`s.

Suite-mode traffic (`HashRequest`/`HashReport`) and `Shutdown` are steady-state
messages after READY, driven per reference-workload §3.2; they never participate in
the boot sequence.

**Error paths (before ring-A `Ready`):** a harness `Fault{code, detail}` at any step,
a `HelloAck` version mismatch, a missing/`layout_version`-mismatched expected region
after the harness's socket-level `Ready`, a unit exit, or a boot-manifest violation
(API.md §7.2) — the agent **never emits `Ready`**. It emits the detail as an agent
`LogLine` (stream 3, level 0; droppable, but the rings are empty this early in
practice), kills the unit if still running and emits `WorkloadExited` (critical) +
doorbell, then powers off via `reboot(RB_POWER_OFF)` (the §4 Shutdown path). The host
observes `WorkloadExited` and a guest-halt exit with no `Ready`, so the orchestrator's
bootstrap `Run(until READY)` fails loudly instead of snapshotting a half-booted guest.
No in-guest retry exists — boot failure must be loud and reproducible. A `Fault`
**after** READY follows reference-workload §3.2's steady-state rule (agent reports
host-ward the same way and halts the VM).

## 5. Memory publication

### Problem

`state-scorer`'s feature map and the hypervisor's `ReadGuestMemory` need to read
specific workload memory (the emulator's emulated-console WRAM array, the framebuffer)
**every exploration step, with zero guest cooperation per read**. Guest-virtual
addresses are useless to the host without walking guest page tables every time; pages
must also be guaranteed not to move or swap.

### Design

1. **Workload side (`detguest-sdk::register_region`)**: the workload calls
   `register_region(name, layout_version, ptr, len, flags)`. The SDK:
   - `mlock2(ptr, len, 0)` — populates and pins every page (no `MLOCK_ONFAULT`; we
     want faults taken *now*, at a deterministic point, not later),
   - touches each page with a volatile read (belt-and-braces prefault check),
   - sends a `RegisterRegion` request to the agent over the agent IPC socket
     (`/run/detguest/agent.sock`, `SOCK_SEQPACKET`) carrying `{name, layout_version,
     gva, len, flags, pid}`.
2. **Agent side (translation)**: the agent reads `/proc/<pid>/pagemap` for
   `[gva, gva+len)`:
   - each 8-byte entry: bit 63 = present, bits 0–54 = PFN. Any non-present or
     swap-flagged page ⇒ registration fails with `RegionError::NotPinned`.
   - guest PFN ⇒ GPA: `gpa = pfn << 12` (the guest kernel's physical address space *is*
     the GPA space the hypervisor exposes — identity by construction of the VM memory
     map).
   - consecutive PFNs are coalesced into extents `{gpa, len}`.
3. **Publication**: the agent writes the region into the **manifest area** of the
   channel page under the seqlock discipline (increment `generation` to odd, write
   entry + extents, increment to even), then emits a `RegionRegister` event (critical)
   on ring A referencing the manifest slot, then doorbells. The host can now resolve
   `name → [extents]` at any time by reading the manifest — including immediately after
   restoring any snapshot, with no event replay needed, because the manifest lives in
   guest RAM and is part of every snapshot.
4. **Host side (`detguest-host::manifest`)**: `Manifest::read(gm)` does a seqlock-
   consistent read (retry while `generation` is odd or changes). `resolve(name)` returns
   `Vec<Extent>` plus `layout_version`. `read_region(gm, name, offset, buf)` walks
   extents and issues `GuestMem::read` per extent — this is the primitive the
   hypervisor's `ReadGuestMemory` and the feature-map reader use.

### Pinning requirements (normative, enforced by the guest image)

- **mlock**: every registered region is mlocked by the SDK before translation. The
  workload's `RLIMIT_MEMLOCK` is set to unlimited by the agent at spawn.
- **No swap in the guest**: the image configures no swap device and no zram. A swapped
  page would change its PFN on swap-in.
- **No page migration**: the guest kernel is built with `CONFIG_COMPACTION=n`,
  `CONFIG_MIGRATION=n`, `CONFIG_KSM=n`, `CONFIG_TRANSPARENT_HUGEPAGE` unset (or boot
  with `transparent_hugepage=never`), `CONFIG_NUMA=n`. mlock alone prevents swap, *not*
  compaction-driven migration — disabling these in the kernel is what makes
  PFN-stability a guarantee instead of a hope.
- **Hugepages (recommendation, not requirement)**: large regions (framebuffers,
  emulator arenas) should be allocated from hugetlbfs (2 MiB pages). hugetlb pages are
  unswappable and unmigratable by construction and produce one manifest extent per
  2 MiB instead of up to 512, making host reads a single contiguous `GuestMem::read`.
  `reference-workload`'s emulator allocates its console-RAM + VRAM + framebuffer arena
  this way (one hugetlbfs mapping, then registers sub-ranges).
- **Stability across snapshot/restore is free**: a snapshot captures all guest RAM and
  vCPU/MMU state; on restore, every GVA→GPA mapping and every manifest byte is
  bit-identical by definition. The pinning rules above only defend against PFN movement
  *within* a single live run. The agent supports a `ReverifyRegions` command (host →
  ring C) that re-walks pagemap and emits `RegionUpdate` if anything moved (a P0 bug if
  it ever fires — it indicates a kernel-config regression).
- **Layout versioning**: `layout_version` is bumped by the workload when the *internal
  structure* of a region changes (e.g., emulator changes its WRAM arena layout).
  `state-scorer` feature maps pin a `layout_version` and the hypervisor refuses feature
  reads on mismatch rather than silently reading garbage.

## 6. Quiesce protocol

### Why it is usually optional

The hypervisor pauses the vCPU **instruction-precisely** (PMU retired-instruction
counter + breakpoint replay, per `determinism-hypervisor`). A snapshot taken at *any*
instruction boundary is complete and consistent at the architectural level: registers,
memory, in-flight kernel state — everything is captured and restores exactly. There is
no "torn" snapshot. Therefore the platform does **not** need guest cooperation to
snapshot, and the default exploration loop never quiesces.

Quiesce exists for **semantic** cleanliness, not correctness:

- **Feature-read tearing**: the scorer reads emulator WRAM while paused; if the pause
  landed mid-frame, derived features (e.g., a 16-bit score being written in two
  stores) may be transiently inconsistent. The cheap fix used by the demo is *frame
  gating*: the emulator calls `frame_mark()` per frame (§2 "Pad input is not on the
  channel") and the orchestrator asks the hypervisor to pause at the next `FRAME_MARK`
  boundary — no quiesce needed. Full quiesce is the heavyweight alternative.
- **Human-friendly snapshot points** for debugging/replay browsing.
- **Workloads with external flush needs** (general bug-hunting mode: flush userspace
  buffers so a snapshot's published regions reflect a consistent application state).

### Protocol (two modes)

**Cooperative (preferred)** — workload links the SDK and calls `quiesce_check()` at its
natural boundaries (the emulator: once per frame):

```
host                      agent                    workload (SDK)
 │  Quiesce{token,COOP} →  │                          │
 │      (ring C, logged)   │  QuiesceReq{token} →     │   (ring I, written by agent)
 │                         │                          │ next quiesce_check():
 │                         │                          │   parks the calling thread
 │                         │  ← QuiesceReady{token} (ring W, critical, + doorbell)
 │  ← drains, sees ready   │                          │
 │  [snapshot here]        │                          │
 │  Resume{token} → ring I │                          │ unparks, returns
```

**Forced (fallback)** — workload does not link the SDK:

```
host: Quiesce{token, FORCED} → ring C
agent: SIGSTOP(workload); waitpid(WUNTRACED); detcall QUIESCE_ACK(token)  ── or ──
       emits QuiesceReady{token} on ring A + doorbell
host: snapshot; Resume{token} → ring C; agent: SIGCONT(workload)
```

Rules:

- `token` is a host-chosen `u64`; `QuiesceReady` must echo it; stale tokens are ignored.
- The host bounds the wait by **virtual time** (e.g., 100 ms guest time). On timeout it
  simply pauses instruction-precisely and snapshots anyway — quiesce is best-effort by
  design.
- All commands and ring writes involved are input-log records, so a replayed run
  quiesces at exactly the same points.

## 7. Determinism rules for everything in-guest (normative)

These bind the agent, the SDK, and any workload that links the SDK. Violations are P0.

1. **No wall-clock or host-time reads.** No `CLOCK_REALTIME`, no NTP, no RTC reads.
   `CLOCK_MONOTONIC` is permitted *only because* the hypervisor virtualizes guest time
   (TSC scaling + logged timer interrupts) — it returns deterministic virtual time. The
   SDK's record timestamps (`vnanos`) come from `CLOCK_MONOTONIC_RAW` for this reason.
2. **No entropy consumption.** The SDK and agent never read `/dev/(u)random`,
   `getrandom(2)`, or `RDRAND`/`RDSEED` (the hypervisor traps/virtualizes these for the
   workload, but SDK/agent code simply must not use them). No `HashMap` default hasher
   (it seeds from randomness) — use `FxHashMap`/`BTreeMap` everywhere in-guest.
3. **Sequence numbers are guest-local counters.** Per-ring record `seq` is a
   monotonically increasing `u32` owned by that ring's producer; the inject seq and
   name-id counters are separate guest-local atomics. Never derived from time or
   addresses.
4. **No ASLR-dependent values on the wire.** The guest boots with
   `norandmaps` / `kernel.randomize_va_space=0`; `norandmaps` arrives via the
   canonical kernel cmdline the hypervisor forces — determinism-hypervisor
   ARCHITECTURE.md §2.3 owns that cmdline and this repo does not restate it (the
   hypervisor seeds boot-time entropy
   deterministically anyway, but addresses still must not leak into event payloads
   except in `RegionRegister`, where the GVA is genuinely meaningful and deterministic
   under disabled ASLR).
5. **Single vCPU; agent single-threaded; SDK thread-safe but order-defined.** With one
   vCPU and fully virtualized interrupts, the guest kernel's scheduling is a
   deterministic function of the input log, so multi-threaded workloads are *allowed*,
   and the SDK's ring producer is guarded by a spinlock — but cross-thread event order
   is owned by kernel scheduling determinism, not by the SDK. v1 guests are single-vCPU
   (MAP.md hypervisor scope).
6. **SDK calls never block on the host except via detcall.** The only waits permitted
   are (a) the critical-event doorbell-and-retry loop and (b) the synchronous
   `inject_point` / quiesce-park paths — all guest-initiated, all replayed at identical
   icounts.
7. **Host writes to channel memory only while the vCPU is paused, and every such write
   is an input-log record.** (§2, restated because it is the load-bearing invariant.)
8. **No floating-point in `detguest-wire`.** Encodings are integer-only; the manifest
   and all payloads are fixed-layout little-endian.
9. **Fallibility must be deterministic.** Any SDK error path (ring full, intern table
   full, manifest full) takes the same branch on replay because its inputs (ring
   occupancy, counter values) are deterministic. Error paths must not consult anything
   outside guest state.

## 8. Event ring record framing (summary)

Full byte-level spec in API.md §3. Every record on every ring:

```
RecordHeader (16 bytes, 8-byte aligned start)
  len    u16   total record length incl. header, multiple of 8
  kind   u8    EventKind / CommandKind / WorkloadCtrlKind (per-ring namespaces)
  flags  u8    bit0 = truncated payload
  seq    u32   per-ring producer counter
  vnanos u64   producer's CLOCK_MONOTONIC_RAW ns (virtual, deterministic);
               0 on host-produced records (host stamps icount in the input
               log instead — guest must not see icount)
[payload]      kind-specific, padded to 8-byte multiple
```

The hypervisor stamps each drained guest event with the drain icount (plus its own
slot/lease identity) on the host side before forwarding to `StreamGuestEvents` — its
`GuestEvent` is `{stream, icount, vns, payload}` (determinism-hypervisor API.md §2);
the hypervisor has no node concept, and node ids are attached orchestrator-side. That
stamp never enters guest memory.
