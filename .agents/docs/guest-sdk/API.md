# guest-sdk API Reference

Normative reference for: the `detguest-sdk` public Rust API (§1), the `detguest-host`
public Rust API (§2), the byte-level event wire format (§3), the region-manifest format
(§4), the detcall PIO register ABI (§5), the agent control-command set (§6), and the
guest boot manifest format (`boot.toml`, §7).

All on-wire integers are **little-endian**. All structures are **fixed-layout** (no
serde on the hot path; `detguest-wire` hand-encodes). Every persisted/shared structure
carries an explicit version field per platform convention.

---

## 1. `detguest-sdk` public API (in-guest, linked by the workload)

```rust
//! detguest-sdk — in-guest instrumentation for the deterministic-execution platform.
//!
//! Every function here is deterministic: no wall-clock reads, no entropy, no
//! unbounded blocking. See ARCHITECTURE.md §7 for the binding rules.

/// One-time initialization. Maps the detchannel from `DETGUEST_CHANNEL_FD`
/// (inherited from detguest-agent), takes ownership of the ring-W producer and
/// ring-I consumer halves, raises the I/O privilege level via `iopl(3)` for the
/// detcall ports (0xD370–0xD39F is above `ioperm(2)`'s 0–0x3FF limit; the process
/// is root with CAP_SYS_RAWIO, and the all-ports grant is security-irrelevant in
/// a trusted lab guest), maps the pv-pad MMIO window via /dev/mem (base GPA
/// 0xD000_1000 — the address is owned by the hypervisor's device map,
/// determinism-hypervisor ARCHITECTURE.md §6.4) for `poll_input`/`frame_mark`,
/// connects to the agent IPC socket, and auto-registers the SDK stats region
/// (`"detsdk.stats"`, layout_version 1) containing the beacon counter array and
/// reachability counters.
///
/// Idempotent; subsequent calls return the existing handle. If the process is not
/// running under detguest-agent (no env var), returns `Err(InitError::NoChannel)`
/// and every other SDK call becomes a deterministic no-op (asserts still evaluate
/// `cond` and panic-on-violation if `DETGUEST_STANDALONE_PANIC=1`), so workloads
/// run unmodified outside the platform.
pub fn init() -> Result<&'static Sdk, InitError>;

#[non_exhaustive]
pub enum InitError {
    NoChannel,
    BadChannelHeader { found_magic: u64 },
    ProtocolVersionMismatch { guest: u32, channel: u32 },
    PioPermissionDenied,       // iopl(3) failed: not root / no CAP_SYS_RAWIO
    AgentSocket(std::io::Error),
}
```

### 1.1 `assert_always`

```rust
/// Record a finding if `cond` is false. A violation is a **critical** event
/// (never dropped): `AssertViolation { name_id, details }` on ring W, doorbell
/// if the ring is full.
///
/// Semantics:
/// - `cond == true`: increments a local per-name pass counter (visible in the
///   "detsdk.stats" region); no ring traffic. Cost: one interned-id lookup +
///   one counter increment.
/// - `cond == false`: emits the event. After `ASSERT_REPEAT_LIMIT` (16)
///   violations of the same `name`, further violations only bump the stats
///   counter and set `flags.truncated` on a final summary event — a broken
///   invariant in a hot loop must not consume the search's event bandwidth.
/// - Never panics, never aborts the workload: the platform's job is to *record*
///   the finding and keep exploring. (The orchestrator decides whether a
///   violating branch is pruned or prioritized.)
///
/// Determinism: `details` is formatted via `fmt::Arguments` — formatting must
/// not read time/entropy/addresses. Truncated to `MAX_DETAILS` (512) bytes.
///
/// `name` must be a `'static` literal; it is interned once (guest-local counter,
/// `NameIntern` event on first use).
pub fn assert_always(cond: bool, name: &'static str, details: fmt::Arguments<'_>);

/// Convenience macro: `det_assert_always!(hp <= max_hp, "hp_within_max", "hp={} max={}", hp, max_hp);`
#[macro_export] macro_rules! det_assert_always { /* ... */ }
```

### 1.2 `expect_reachable`

```rust
/// Declare "search should be able to get here", and record that it did.
///
/// Semantics:
/// - First hit per name per process lifetime: emits critical event
///   `Reachable { name_id }` on ring W.
/// - Subsequent hits: increment the per-name hit counter in "detsdk.stats"
///   (no ring traffic).
/// - The *absence* of a `Reachable` for a declared name across an entire search
///   campaign is itself a signal ("declared but never reached"). To make the
///   declaration visible even when never hit, the name is interned (and thus a
///   `NameIntern` event emitted) at first *call site execution*; for
///   never-executed sites, workloads should pre-declare in their init path:
pub fn expect_reachable(name: &'static str);

/// Pre-declare a reachability target without hitting it (emits `NameIntern`
/// with the REACHABLE_DECL flag so the orchestrator knows the universe of
/// targets up front).
pub fn declare_reachable(name: &'static str);
```

### 1.3 `coverage_beacon`

```rust
/// Cheap coverage counter for the scorer. `id` indexes a fixed array of
/// `BEACON_SLOTS` (65_536) `u32` saturating counters inside the auto-registered
/// "detsdk.stats" region; the host/scorer reads the array directly via memory
/// publication — **zero ring traffic** on the hot path.
///
/// Ring traffic: a droppable `Beacon { id }` event is emitted only on the first
/// hit of each id (discovery signal for the orchestrator's coverage frontier).
///
/// `id >= BEACON_SLOTS` is masked (`id & 0xFFFF`); workloads should keep ids
/// dense and stable across builds (they are features keyed by value).
///
/// Cost target: < 10 ns when already hit (one relaxed atomic increment).
pub fn coverage_beacon(id: u32);

pub const BEACON_SLOTS: usize = 65_536;
```

### 1.4 `inject_point`

```rust
/// Ask the host whether to inject a fault here. The decision comes from the
/// **input log** (during exploration: from the fault plan the input-synthesizer
/// attached to this burst, which the hypervisor records into the log; during
/// replay: read back from the log), so the same call site in the same execution
/// gets the same answer, bit-for-bit.
///
/// Mechanics (see INTEGRATION.md for the full round trip):
/// 1. allocate `iseq` from the guest-local inject counter,
/// 2. write critical event `InjectQuery { iseq, name_id }` to ring W,
/// 3. detcall: `OUT 0xD384, eax = iseq` then `IN eax, 0xD384`,
/// 4. decode eax as a packed `FaultDecision`.
///
/// The detcall is a synchronous, guest-initiated VM exit — the host drains
/// ring W inside the exit, matches `iseq`, consults plan/log, records the
/// answer in the input log, and returns it in `eax`. Cost: one VM exit
/// (~microseconds). Place inject points at I/O boundaries, not inner loops.
///
/// Returns `FaultDecision::Proceed` when not under the platform (standalone),
/// when no plan covers this point, or on any channel error — failure to decide
/// is never a fault.
pub fn inject_point(name: &'static str) -> FaultDecision;

/// Packed into 32 bits on the wire: bits 0..8 = kind, bits 8..32 = arg (u24).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaultDecision {
    /// kind 0 — no fault; take the normal path.
    Proceed,
    /// kinds 1–63: platform-defined. arg semantics per kind:
    ///   1 = FailGeneric  (arg = suggested errno),
    ///   2 = ShortCount   (arg = max bytes/items to process),
    ///   3 = DelayVirtual (arg = milliseconds of *virtual* time to sleep).
    Platform { kind: u8, arg: u32 },
    /// kinds 64–255: workload-defined; semantics documented by the workload's
    /// fault-plan schema (input-synthesizer treats them opaquely).
    Workload { kind: u8, arg: u32 },
}
```

### 1.5 Memory publication

```rust
bitflags::bitflags! {
    pub struct RegionFlags: u32 {
        /// Host treats contents as a framebuffer (hint for state-scorer GPU path).
        const FRAMEBUFFER = 1 << 0;
        /// Contents change every step; hypervisor may include in per-step feature reads.
        const HOT         = 1 << 1;
        /// Host may *write* this region (reserved; v1 host never writes regions).
        const HOST_WRITABLE = 1 << 2;
    }
}

/// Publish `[ptr, ptr+len)` to the host under `name`.
///
/// The SDK mlocks and prefaults the range, then asks the agent to translate
/// GVA→GPA via /proc/<pid>/pagemap and publish the extents in the region
/// manifest (ARCHITECTURE.md §5). Blocks (deterministically, on the agent IPC
/// socket) until the manifest is updated and the `RegionRegister` event is on
/// ring A.
///
/// Requirements on the memory:
/// - lifetime: must outlive the registration (until `unregister` or exit);
///   the returned handle unregisters on drop.
/// - stability: must not be freed/realloc'd/remapped while registered. Growable
///   containers (`Vec`) are forbidden; use a fixed mmap or `Box<[u8; N]>`.
/// - prefer one hugetlbfs arena for large regions (1 extent per 2 MiB).
///
/// `layout_version`: bump when the internal layout of the region changes;
/// feature maps bind to (name, layout_version).
///
/// Errors: `ManifestFull` (64 regions max), `TooManyExtents` (region would
/// exceed the manifest extent pool), `NotPinned` (pagemap shows non-present or
/// swapped pages), `NameTooLong` (> 56 bytes), `AgentUnavailable`.
///
/// # Safety
/// `ptr..ptr+len` must remain valid, mapped, and non-relocating for the life
/// of the returned handle. The host reads it asynchronously at any time.
pub unsafe fn register_region(
    name: &'static str,
    layout_version: u32,
    ptr: *const u8,
    len: usize,
    flags: RegionFlags,
) -> Result<RegionHandle, RegionError>;

pub struct RegionHandle { /* region_id: u32 */ }
impl RegionHandle {
    pub fn region_id(&self) -> u32;
    /// Explicit unregister (also done on Drop): agent marks the manifest entry
    /// dead (flags bit 31), bumps generation, emits RegionUpdate.
    pub fn unregister(self);
}
```

### 1.6 Controller input + frame boundary (pv-pad latch)

Pad input has exactly one delivery path: the hypervisor's pv-pad MMIO latch
(ARCHITECTURE.md §2 "Pad input is not on the channel"). Ring I never carries pad data.

```rust
/// Per-frame controller read: a thin wrapper over a 32-bit MMIO read of the
/// hypervisor's pv-pad latch for pad `port` (0–3). The register addresses
/// (PAD0..PAD3 at base GPA 0xD000_1000 + 0x08 + 4*port) come from the
/// hypervisor's MMIO device map — determinism-hypervisor ARCHITECTURE.md §6.4
/// owns them; this crate only wraps the read.
///
/// Call exactly once per emulated frame (the hypervisor's contract: read the
/// latch once per frame; never cache across frames). The read is an MMIO VM
/// exit at a deterministic icount, and the latch changes only when a canonical
/// `PAD_SET` input-log record lands at its icount — so the returned value is
/// bit-deterministic on replay.
///
/// Returns the current button bitmask. Bit semantics are owned by
/// reference-workload's pad mapping (its feature-map/pad documents), opaque
/// here. Standalone mode (no platform): returns 0.
pub fn poll_input(port: u8) -> u32;

/// Frame-boundary mark. Call exactly once per emulated frame, after the frame
/// is fully computed/blitted:
/// 1. writes critical event `FrameMark { frame_index }` to ring W and
///    release-stores the producer index,
/// 2. MMIO-writes the incremented frame index to pv-pad `FRAME_COUNTER`.
/// The FRAME_COUNTER write is the frame-boundary VM exit: the host records
/// `frame → icount` there, and the hypervisor's `at_frame` input scheduling and
/// `next_sdk_event` stop conditions key off this FRAME_MARK pair (its API.md).
/// The ring record precedes the MMIO write so a host draining inside the exit
/// always sees it (same ordering discipline as InjectQuery before OUT 0xD384).
///
/// This is the platform's only frame-boundary signal — there is no separate
/// "frame-end beacon" id convention.
pub fn frame_mark();
```

### 1.7 Quiesce

```rust
/// Cooperative quiesce point. Call at semantically clean boundaries (the
/// emulator calls this once per emulated frame). This call also services
/// ring I (the host→workload control ring): it consumes any pending
/// `QuiesceReq`/`Resume` records. If a `QuiesceReq{token}` is pending: emit
/// critical `QuiesceReady{token}` on ring W, doorbell, then park the calling
/// thread in a PIO-free spin-yield loop until `Resume{token}` arrives on
/// ring I. Otherwise: ~3 ns (one relaxed load when ring I is empty).
pub fn quiesce_check();
```

### 1.8 Misc

```rust
/// Structured log line host-ward (droppable; stream id 4 = "sdk user").
pub fn log_line(level: LogLevel, msg: &str);

/// Snapshot of local SDK statistics (same data the host reads from the
/// "detsdk.stats" region — see §4.3 for that region's layout).
pub fn stats() -> SdkStats;
```

---

## 2. `detguest-host` public API (host, linked by determinism-hypervisor)

```rust
/// Hypervisor-provided access to guest physical memory. Implemented by the
/// VMM over its memslot mappings. All offsets are GPAs.
pub trait GuestMem {
    fn read(&self, gpa: u64, buf: &mut [u8]) -> Result<(), MemError>;
    fn write(&mut self, gpa: u64, buf: &[u8]) -> Result<(), MemError>;
}

/// Hook through which EVERY host-side mutation of channel memory is reported,
/// so the hypervisor can append it to the input log (ARCHITECTURE.md §2).
/// `icount` is supplied by the hypervisor at call time.
pub trait ChannelWriteSink {
    fn ring_push(&mut self, ring: RingId, bytes: &[u8], new_prod: u32);
    fn cons_bump(&mut self, ring: RingId, new_cons: u32);
    fn pio_answer(&mut self, port: u16, value: u32);
}

pub struct Channel<M: GuestMem> { /* base GPA, validated header */ }

impl<M: GuestMem> Channel<M> {
    /// Attach after the guest's CHANNEL_INIT detcall. Validates magic,
    /// proto_version (==1), ring descriptors (within the 2 MiB page, power-of-
    /// two sizes), and returns Err(AttachError) otherwise — the PIO handler
    /// turns that into a nonzero init status for the guest's `IN 0xD37C`.
    pub fn attach(gm: M, base_gpa: u64) -> Result<Self, AttachError>;

    /// Drain all complete records from rings A and W. Bumps consumer indices
    /// through `sink`. Call ONLY while the vCPU is paused (pause boundary or
    /// inside a PIO exit). Returns events in (ring, seq) order; the caller
    /// stamps them with the drain icount (plus its own slot/lease identity —
    /// the hypervisor has no node concept; node ids are orchestrator-side).
    pub fn drain_events(&mut self, sink: &mut dyn ChannelWriteSink)
        -> Result<Vec<GuestEvent>, WireError>;

    /// Push a command (ring C) or a workload-control record (ring I — quiesce
    /// relay only in v1; NEVER pad input, which travels via the pv-pad latch).
    /// Errors if the ring lacks space — the host, unlike the guest, may simply
    /// wait and retry at the next pause; it never spins the guest.
    pub fn push_command(&mut self, cmd: &Command, sink: &mut dyn ChannelWriteSink)
        -> Result<(), PushError>;
    pub fn push_workload_ctrl(&mut self, rec: &WorkloadCtrl, sink: &mut dyn ChannelWriteSink)
        -> Result<(), PushError>;

    /// Seqlock-consistent manifest snapshot.
    pub fn read_manifest(&self) -> Result<RegionManifest, WireError>;

    /// Resolve + read a published region: walks extents, concatenates.
    /// This is what the hypervisor's ReadGuestMemory(region=..) delegates to.
    pub fn read_region(&self, name: &str, offset: u64, buf: &mut [u8])
        -> Result<(), RegionReadError>;

    /// Current drop counters (guest-written; read-only here).
    pub fn drop_counters(&self) -> DropCounters;
}

/// Answers inject_point detcalls. The hypervisor's PIO handler calls
/// `answer(iseq)`; this looks up the matching InjectQuery (drained just before,
/// inside the same exit), asks the FaultPlan, and returns the packed u32.
pub struct InjectResponder<P: FaultPlan> { /* .. */ }

/// Recording mode: implemented over the input-synthesizer-provided fault plan
/// for the burst; the hypervisor records (iseq, decision) into the input log.
/// Replay mode: implemented over the input log itself (decisions read back).
pub trait FaultPlan {
    fn decide(&mut self, iseq: u32, name_id: u32, name: Option<&str>) -> FaultDecision;
}

/// A drained, typed guest event plus its host stamp.
pub struct GuestEvent {
    pub ring: RingId,          // A or W
    pub seq: u32,
    pub vnanos: u64,           // guest virtual time (deterministic)
    pub truncated: bool,
    pub payload: EventPayload, // enum, §3.2
}
```

`detguest-host` performs **interning bookkeeping** for the hypervisor: it folds
`NameIntern` events into an id→string table per channel so `GuestEvent` consumers can
resolve `name_id`s; the table must be checkpointed alongside the hypervisor's
per-branch state (it is reconstructible from the event stream, but caching it avoids
re-scans).

---

## 3. Event wire format (byte level)

### 3.0 Record framing (all four rings)

Records start 8-byte aligned and never wrap (tail too small ⇒ `Pad` record, kind 0,
`len` = remaining tail bytes).

```
offset  size  field    notes
0       2     len      u16, total record bytes incl. this header; multiple of 8;
                       16 ≤ len ≤ 4096
2       1     kind     namespace depends on ring (EventKind for A/W,
                       CommandKind for C, WorkloadCtrlKind for I)
3       1     flags    bit0 TRUNCATED  bit1 REACHABLE_DECL (NameIntern only)
4       4     seq      u32 per-ring producer counter, starts at 0
8       8     vnanos   u64 guest CLOCK_MONOTONIC_RAW ns; 0 for host-produced
16      ...   payload  kind-specific, zero-padded to 8-byte multiple
```

### 3.1 EventKind (rings A and W)

| kind | name | class | producer |
|---|---|---|---|
| 0 | `Pad` | — | any |
| 1 | `Hello` | critical | agent |
| 2 | `NameIntern` | critical | agent, SDK |
| 3 | `AssertViolation` | critical | SDK |
| 4 | `Reachable` | critical (first hit only is sent) | SDK |
| 5 | `Beacon` | droppable (first hit only is sent) | SDK |
| 6 | `InjectQuery` | critical | SDK |
| 7 | `RegionRegister` | critical | agent |
| 8 | `RegionUpdate` | critical | agent |
| 9 | `WorkloadStarted` | critical | agent |
| 10 | `WorkloadExited` | critical | agent |
| 11 | `LogLine` | droppable | agent, SDK |
| 12 | `QuiesceReady` | critical | agent, SDK |
| 13 | `FrameMark` | critical | SDK |
| 14 | `Ready` | critical | agent |

Critical ⇒ on full ring: doorbell + retry. Droppable ⇒ on full ring: bump
`dropped_records`/`dropped_bytes`/`dropped_by_kind[kind]` in the channel header, skip.

### 3.2 Event payloads

```
Hello (kind 1)                       payload 16 bytes
  0  u32  proto_version  (=1, must match header)
  4  u32  agent_version  (crate semver packed: major<<16|minor<<8|patch)
  8  u64  capabilities   bit0 FORCED_QUIESCE  bit1 REVERIFY_REGIONS

NameIntern (kind 2)                  payload 8 + name, padded
  0  u32  name_id        from the guest-local intern counter, starts at 1
  4  u16  name_len       ≤ 256
  6  u16  _pad
  8  ..   name bytes (UTF-8, no NUL)
  flags.REACHABLE_DECL set when emitted by declare_reachable().

AssertViolation (kind 3)             payload 16 + details, padded
  0  u32  name_id
  4  u32  violation_count   per-name count incl. this one (1-based)
  8  u16  details_len       ≤ 512 (flags.TRUNCATED if clipped)
  10 u16  _pad
  12 u32  _pad2
  16 ..   details bytes (UTF-8)

Reachable (kind 4)                   payload 8 bytes
  0  u32  name_id
  4  u32  _pad             (hit counts live in the detsdk.stats region)

Beacon (kind 5)                      payload 8 bytes
  0  u32  beacon_id        (< 65536; first hit of this id only)
  4  u32  _pad

InjectQuery (kind 6)                 payload 8 bytes
  0  u32  iseq             guest-local inject counter, starts at 0
  4  u32  name_id

RegionRegister (kind 7) / RegionUpdate (kind 8)   payload 16 bytes
  0  u32  region_id        manifest slot index
  4  u32  name_id
  8  u32  layout_version
  12 u32  manifest_generation   (even value after the update)
  Full extents live in the manifest; the event is a notification + pointer.

WorkloadStarted (kind 9)             payload 8 bytes
  0  u32  guest_pid
  4  u32  unit             which preconfigured workload entry was launched

WorkloadExited (kind 10)             payload 16 bytes
  0  u32  guest_pid
  4  i32  exit_code        (-1 if killed by signal)
  8  i32  term_signal      (0 if normal exit)
  12 u32  _pad

LogLine (kind 11)                    payload 8 + msg, padded
  0  u8   stream           1 stdout, 2 stderr, 3 agent, 4 sdk-user
  1  u8   level            0 error … 4 trace
  2  u16  msg_len          ≤ 1024 (flags.TRUNCATED if clipped)
  4  u32  _pad
  8  ..   msg bytes (UTF-8, invalid sequences lossily replaced by producer)

QuiesceReady (kind 12)               payload 8 bytes
  0  u64  token            echo of the host's Quiesce token

FrameMark (kind 13)                  payload 8 bytes
  0  u32  frame_index      emulated video frame just completed; equals the
                           value MMIO-written to pv-pad FRAME_COUNTER
                           immediately after this record (§1.6 ordering rule)
  4  u32  _pad

Ready (kind 14)                      payload 16 bytes
  0  u32  unit             autostart unit started (0xFFFF_FFFF if none)
  4  u32  region_count     live regions in the manifest at emit time
  8  u64  manifest_generation   manifest seqlock generation (even)
  The deterministic READY point — fires per ARCHITECTURE.md §4.1; the
  orchestrator's bootstrap keys its root snapshot on this event.
```

### 3.3 CommandKind (ring C, host → agent)

Same 16-byte record header (`vnanos` = 0; the input log carries the icount).

```
1  StartWorkload      payload 8:  u32 unit; u32 log_mask
2  Quiesce            payload 16: u64 token; u32 mode (0 COOP, 1 FORCED); u32 _pad
3  Resume             payload 8:  u64 token          (FORCED path; COOP Resume rides ring I)
4  Shutdown           payload 8:  u32 mode (0 graceful, 1 immediate); u32 _pad
5  SetLogMask         payload 8:  u32 mask; u32 _pad
6  ReverifyRegions    payload 0
```

### 3.4 WorkloadCtrlKind (ring I, host → workload/SDK)

Ring I is the host→workload **control** ring: quiesce relay only in v1, reserved for
future workload-directed command/fault records. It carries **no pad data** — pad input
travels exclusively via the hypervisor's pv-pad MMIO latch (§1.6, ARCHITECTURE.md §2).

```
1  (reserved)         was the generic opaque Input record; removed — pad input
                      never rides ring I. Kind 1 is never reassigned in v1.
2  QuiesceReq         payload 8:  u64 token
3  Resume             payload 8:  u64 token
```

### 3.5 Versioning rules

- `proto_version` (channel header + Hello) gates everything; v1 is this document.
- Adding an EventKind is backward-compatible (unknown kinds are skipped by `len`,
  counted in a host metric). Changing any existing payload layout requires bumping
  `proto_version`. Golden tests pin every byte of every v1 payload.

---

## 4. Region manifest format (channel page, offset 0x1000, 28 KiB)

### 4.1 Layout

```
ManifestHeader (offset 0x1000)
  0   u32  magic            = 0x4644_5444   ("DTDF")
  4   u16  manifest_version = 1
  6   u16  region_capacity  = 64
  8   u64  generation       seqlock: odd while agent is writing; readers retry
  16  u32  region_count     live entries (dead entries keep slots; see flags)
  20  u32  extent_count     used slots in the extent pool
  24  u64  _reserved

RegionEntry[64]  (offset 0x1020, 96 bytes each)
  0   u32  region_id        == slot index; stable for the life of the channel
  4   u32  name_id          intern id (string arrives via NameIntern event;
                            ALSO inlined below so the manifest is self-contained
                            after a bare snapshot restore)
  8   u32  layout_version
  12  u32  flags            RegionFlags bits; bit31 = DEAD (unregistered)
  16  u64  gva              guest-virtual base (informational/debug)
  24  u64  len              bytes
  32  u32  extent_off       index into the extent pool
  36  u32  extent_n         number of extents
  40  u8[56] name           UTF-8, NUL-padded (hard cap 56 bytes)

ExtentPool[1024] (offset 0x2820, 16 bytes each)
  0   u64  gpa
  8   u64  len              bytes; extents of one region are logically
                            concatenated in order
```

Total: 0x1020 + 64×96 + 1024×16 = 0x2820 + 0x4000 = 0x6820 < 0x8000 (fits the
manifest area with room for v2 growth).

### 4.2 Writer/reader discipline

- **Writer (agent only)**: `generation += 1` (→ odd), full fence, mutate, full fence,
  `generation += 1` (→ even). One writer ever; no CAS needed.
- **Reader (host)**: read `generation` (even or retry), copy what it needs, re-read
  `generation`; retry on change. The host reads while the vCPU is paused in practice,
  so retries only occur if the pause landed mid-registration.
- After **snapshot restore**, the manifest is immediately valid (it is guest RAM); the
  host re-reads it instead of replaying registration events.

### 4.3 The auto-registered `"detsdk.stats"` region (layout_version 1)

```
0x00000  u32  stats_version (=1)
0x00004  u32  _pad
0x00008  u64  asserts_passed_total
0x00010  u64  asserts_failed_total
0x00018  u64  reachable_names           count of distinct names hit
0x00020  u64  inject_queries_total
0x00040  u32 beacon_counts[65536]       saturating; index = beacon id
0x40040  { u32 name_id, u32 hits }[1024]  reachable hit table (insertion order)
0x42040  { u32 name_id, u32 pass_lo, u32 fail_lo, u32 _pad }[1024] assert table
```

Size: 0x46040 (~280 KiB), allocated by the SDK from a private hugetlbfs mapping at
`init()`. `state-scorer` consumes `beacon_counts` directly as features.

---

## 5. detcall PIO register ABI

All accesses are 32-bit (`OUT eax / IN eax`). Unknown ports in the range are RAZ/WI.
Every access is a VM exit handled synchronously by the hypervisor with the vCPU paused;
every `IN` return value is recorded in the input log (`ChannelWriteSink::pio_answer`,
serialized by the hypervisor as a DHILOG `DEV_EVENT` record — payload encodings are
defined in determinism-hypervisor's API.md, not here).

| Port | OUT (guest→host) | IN (host→guest) |
|---|---|---|
| `0xD370` IDENT | — | `0xD37E0001` = magic `0xD37E` ‹‹16 \| proto 1 |
| `0xD374` INIT_LO | channel GPA bits 0–31 (latched) | last latched value |
| `0xD378` INIT_HI | channel GPA bits 32–63 (latched) | last latched value |
| `0xD37C` INIT_GO | commit: eax = channel size in 4 KiB pages (512); host validates + attaches | status: 0 OK, 1 bad GPA, 2 bad magic/version, 3 already attached |
| `0xD380` DOORBELL | eax = ring mask (bit0 ring A, bit1 ring W): host drains those rings now (drain + cons bump logged) | 0 |
| `0xD384` INJECT | eax = iseq (selects the pending query; host drains ring W first if the matching InjectQuery is not yet seen) | packed FaultDecision for the selected iseq: bits 0–7 kind, bits 8–31 arg. `0` = Proceed. Unmatched iseq ⇒ `0` (Proceed) + host warning metric |
| `0xD388` QUIESCE_ACK | eax = low 32 bits of token (FORCED path; full token already known to host) | 0 |

Sequencing rule for INJECT: the SDK must write the `InjectQuery` record and
release-store the ring-W producer index **before** `OUT 0xD384` — the host's PIO
handler drains ring W inside the exit and must find the query.

---

## 6. Agent control-command summary (semantics)

| Command | Agent behavior | Acknowledgement |
|---|---|---|
| `StartWorkload{unit, log_mask}` | Fork+exec the image-baked workload entry `unit`; wire pipes; set `DETGUEST_CHANNEL_FD`, `RLIMIT_MEMLOCK=∞`; apply `log_mask` | `WorkloadStarted` event (ring A) |
| `Quiesce{token, COOP}` | Relay `QuiesceReq{token}` onto ring I | `QuiesceReady{token}` from SDK (ring W) |
| `Quiesce{token, FORCED}` | `SIGSTOP` workload, `waitpid(WUNTRACED)` | `QuiesceReady{token}` from agent (ring A) or detcall `QUIESCE_ACK` |
| `Resume{token}` (ring C) | `SIGCONT` (FORCED path) | none (host observes execution continue) |
| `Resume{token}` (ring I) | consumed by SDK: unpark from `quiesce_check` | none |
| `Shutdown{mode}` | graceful: SIGTERM workload, 2 s virtual-time grace, then SIGKILL, emit `WorkloadExited`, `reboot(RB_POWER_OFF)`; immediate: skip grace | `WorkloadExited` + VM power-off exit |
| `SetLogMask{mask}` | adjust which LogLine levels/streams are produced (reduces droppable traffic) | none |
| `ReverifyRegions` | re-walk pagemap for all live regions; emit `RegionUpdate` per region whose extents changed (P0 alarm) or are unchanged (generation echo) | `RegionUpdate` events |

Boot-time autostart (the boot manifest's configured unit — §7) reuses the
`StartWorkload` code path agent-locally with **no** ring-C record, and — for a unit
that declares a control protocol — drives the harness control protocol through
`Start{}` (ARCHITECTURE.md §4.2) before emitting `Ready`, so no host input precedes
the `Ready` event — see ARCHITECTURE.md §4.1 for the READY-point contract the
orchestrator's bootstrap keys on.

Host-side rule (hypervisor): commands are pushed only while the vCPU is paused, each
push is an input-log record (DHILOG `DEV_EVENT`; encodings in determinism-hypervisor
API.md), and the push is considered delivered when the guest's next ring-C poll
consumes it — the host never spins waiting for command consumption; it checks
acknowledgement events at subsequent pauses.

---

## 7. Guest boot manifest (`/etc/detguest/boot.toml`)

This repo **owns the format**; the agent is its only parser. The file lives at
`/etc/detguest/boot.toml` inside the **initramfs** and is baked in by the image build
(`reference-workload`'s `xtask image` for the demo — its API.md §4 manifest pins the
initramfs hash, so the boot manifest is immutable image content). That immutability is
load-bearing: the READY-point icount is a pure function of the WorkloadImage
(ARCHITECTURE.md §4.1) precisely because everything the agent does at boot — including
this file's contents — ships inside the image. The kernel **cmdline is not configured
here**: the hypervisor forces the canonical deterministic cmdline
(determinism-hypervisor ARCHITECTURE.md §2.3 owns it); `boot.toml` configures the
agent only.

### 7.1 Schema (TOML, version-prefixed per platform convention)

```toml
boot_toml_version = 1            # required; the agent rejects unknown majors loudly
                                 #   (no Ready, fault path — §7.3)

[autostart]                      # optional. Absent => no autostart: Ready fires
unit = 0                         #   immediately after Hello with region_count = 0
                                 #   (ARCHITECTURE.md §4.1). `unit` selects a
                                 #   [[unit]] entry by id.

[[unit]]                         # 1..N preconfigured workload entries. Ring-C
id = 0                           #   StartWorkload{unit} and [autostart] select by
exec = "/usr/bin/refwork-harness"#   `id` (dense, from 0). argv is NEVER sent over
args = ["--config", "/etc/refwork/harness.toml"]  # the wire (§6) — it lives here.
log_mask = 0x1F                  # optional; initial LogLine mask (SetLogMask overrides)

[unit.control]                   # optional. Present => the unit speaks the harness
protocol = "refwork-ctl"         #   control protocol and the agent drives its leg
proto_version = 1                #   (ARCHITECTURE.md §4.2). Message set, framing and
game_dev = "/dev/vdb"            #   transport are reference-workload API.md §3's
                                 #   (postcard CtlMsg over socketpair(AF_UNIX,
                                 #   SOCK_SEQPACKET), child end inherited as fd 3;
                                 #   proto_version pinned here must match §3.1).
                                 #   `game_dev` is the LoadGame.dev_path the agent
                                 #   sends (the virtio-blk game image device).

[[expected_region]]              # 0..N. The READY gate (ARCHITECTURE.md §4.1/§4.2):
name = "wram"                    #   Ready is withheld until every listed region is
layout_version = 1               #   live in the channel manifest (§4) with EXACTLY
                                 #   this layout_version.
[[expected_region]]
name = "framebuffer"
layout_version = 1

[[expected_region]]
name = "meta"
layout_version = 1
```

The example shows the demo image's values: unit 0 is reference-workload's harness, and
the expected regions are its always-published set `wram`/`framebuffer`/`meta`
(reference-workload API.md §4 `regions` — that repo owns the names and bakes this file
into its image).

### 7.2 Field rules (normative)

- `boot_toml_version`: required integer; same major/minor convention as
  `proto_version` (§3.5) — unknown major ⇒ boot fault.
- `[[unit]]`: `id` values dense from 0 and unique; `exec` an absolute path inside the
  image; `args` optional (default empty). A unit **without** `[unit.control]` is
  started bare (fork+exec only; no fd 3 is passed).
- `[unit.control]`: `protocol` is an open identifier (v1 defines only `refwork-ctl` =
  reference-workload API.md §3); `proto_version` must equal the value the agent speaks;
  `game_dev` required for `refwork-ctl`.
- `[[expected_region]]`: `name` ≤ 56 bytes (the manifest cap, §4.1); `layout_version`
  must match the manifest entry exactly — a mismatch is a boot fault (§7.3), never a
  silent downgrade. The list may be empty (e.g. the trivial M2 test workload).
- `[autostart]` referencing a nonexistent unit id, duplicate region names, or any parse
  error: boot fault.

### 7.3 Boot fault path

Any §7.2 violation, and any failure of the §4.2 protocol leg before `Ready`
(ARCHITECTURE.md §4.2 error rules): the agent never emits `Ready`; it emits the detail
as an agent `LogLine`, emits `WorkloadExited` (critical) if a unit was running, and
powers off (`reboot(RB_POWER_OFF)`). The orchestrator's bootstrap `Run(until READY)`
then fails loudly instead of snapshotting a half-booted guest.
