# guest-sdk Integration Guide

How the sister services from [`../MAP.md`](../MAP.md) consume this repo. Contracts here
bind both sides; schema changes follow the versioning rules in API.md §3.5.

## 1. determinism-hypervisor: channel driver + `StreamGuestEvents`

The hypervisor links `detguest-host` and owns four touchpoints:

1. **PIO handler** (`KVM_EXIT_IO`, ports `0xD370–0xD39F`): routes to
   `Channel::attach` (INIT_GO), `Channel::drain_events` (DOORBELL),
   `InjectResponder::answer` (INJECT), and the quiesce tracker (QUIESCE_ACK). Every
   `IN` answer goes through `ChannelWriteSink::pio_answer` into the input log.
2. **Pause-boundary drain**: at the end of every exploration burst (and at any
   instruction-precise pause), the hypervisor calls `drain_events` and stamps each
   `GuestEvent` with the drain icount (plus its own slot/lease identity — the
   hypervisor has no node concept; node ids are attached orchestrator-side).
3. **Command/control push + pad landing**: channel pushes are commands (ring C) and
   workload-control records (ring I — quiesce relay only; never pad data), performed
   only while the vCPU is paused and each logged by the hypervisor as a DHILOG
   `DEV_EVENT` record (payload encodings defined in its API.md). Pad input never rides
   the channel: the orchestrator's "apply this input burst" becomes a schedule of
   canonical `PAD_SET` log records the hypervisor lands into the **pv-pad MMIO latch**
   at the scheduled icounts/frames (`at_frame` resolves via the guest's FRAME_MARK
   pair — hypervisor ARCHITECTURE.md §6.4 and API.md). The guest observes inputs
   solely via `poll_input()`'s once-per-frame latch read; replay lands the same
   `PAD_SET`s at the same icounts, so every read returns identical values.
4. **`StreamGuestEvents`** (hypervisor gRPC — the message and RPC are owned by
   determinism-hypervisor's API.md §2, not restated here): a server-streaming RPC its
   clients subscribe to per lease. The hypervisor converts `detguest_host::GuestEvent`
   1:1 into its proto `GuestEvent { stream, icount, vns, payload }` — `stream` is the
   detchannel `EventKind` (API.md §3.1), `icount` is the host drain stamp, and
   `payload` carries the record payload bytes whose framing this repo owns (API.md
   §3.2; consumers decode via `detguest-wire`, resolving `name_id`s through the intern
   table `detguest-host` maintains). There is no node or branch identity in the
   message — node ids exist only orchestrator-side.

5. **`ReadGuestMemory(region, offset, len)`**: the hypervisor's existing GPA-read RPC
   gains a by-name mode that delegates to `Channel::read_region` — manifest lookup,
   extent walk, `GuestMem::read` per extent. Reads happen only while the branch's vCPU
   is paused, so no tearing beyond what quiesce/beacon-gating addresses.

**Snapshot/restore note:** the channel needs *no* hypervisor-side serialization. All
channel state is guest RAM; after restore the hypervisor re-runs `Channel::attach` at
the same base GPA (the GPA is part of the hypervisor's per-VM metadata, recorded once
at CHANNEL_INIT) and re-reads the manifest. The intern table is rebuilt from replayed
events or checkpointed alongside hypervisor branch metadata.

## 2. reference-workload: the emulator as an SDK client

**Bring-up (who drives what).** The emulator harness is exec'd by the agent as the
boot manifest's autostart unit (`/etc/detguest/boot.toml` — format owned by this repo,
API.md §7; reference-workload bakes the file into its image with the demo unit,
`game_dev`, and the expected-regions list `wram`/`framebuffer`/`meta`). The **agent**
then drives the control-protocol leg `Hello → LoadGame → Start` over fd 3
(ARCHITECTURE.md §4.2; message set and framing owned by reference-workload API.md §3),
and emits the ring-A `Ready` event only after `Start` succeeded and every expected
region is live — so the READY-point root snapshot captures the harness already inside
its free-running loop.

The deterministic console emulator links `detguest-sdk` and does, at startup:

```rust
let _sdk = detguest_sdk::init()?;

// One hugetlbfs arena, carved into published regions. Region names are
// workload-defined strings declared in the WorkloadImage manifest's `regions`
// list (reference-workload API.md §4); these are the demo's canonical names:
//   "wram"          128 KiB  emulated work RAM   (HOT)
//   "framebuffer"   224 KiB  decoded framebuffer (HOT | FRAMEBUFFER)
//   (plus "meta" always, and optional "vram"/"sram" — reference-workload §3.5)
let wram = unsafe { register_region("wram",        1, arena.wram_ptr(), 0x20000, RegionFlags::HOT)? };
let fb   = unsafe { register_region("framebuffer", 1, arena.fb_ptr(),  0x38000, RegionFlags::HOT | RegionFlags::FRAMEBUFFER)? };
```

Per emulated frame:

```rust
let pad = detguest_sdk::poll_input(0);  // pv-pad latch read — one MMIO exit (API.md §1.6)
emulator.set_pad(0, pad);
emulator.run_one_frame();
detguest_sdk::frame_mark();             // FrameMark on ring W + pv-pad FRAME_COUNTER++
detguest_sdk::quiesce_check();
```

**Pad input path (binding decision).** Pad state reaches the workload **only** via the
hypervisor's pv-pad MMIO latch; `poll_input()` is a wrapper over that latch read
(API.md §1.6; latch addresses owned by the hypervisor's device map, its
ARCHITECTURE.md §6.4). reference-workload's former `FrameInput` socket message is
**removed** in favor of direct SDK latch reads — the emulator harness, linking the SDK,
reads the latch itself once per frame; nothing relays pad bytes over sockets or ring I
(reference-workload cites this decision). Button-bit semantics of the `u32` latch value
are owned by reference-workload's pad mapping; guest-sdk treats them as opaque.

`input-synthesizer` emits bursts as `[(frame_offset, pad_state)]`; the orchestrator
hands them to the hypervisor, which compiles each entry into a canonical `PAD_SET`
input-log record landed into the latch at the scheduled frame (`at_frame` → icount via
the FRAME_MARK table; hypervisor API.md). The emulator consumes the latch 1:1 with
frames — one read per frame, by contract.

**Goal/progress signals** come from RAM, not events: `state-scorer` reads
`wram` via the feature map. The emulator additionally calls
`expect_reachable("credits_sequence_entered")` at the end-credits decoder as a
belt-and-braces terminal signal, and `assert_always(save_ram_checksum_ok, ...)` as an
emulator-integrity invariant.

## 3. state-scorer: feature map over published regions

The feature map is owned by `reference-workload` — its API.md §1 is the platform-wide
canonical schema, and this excerpt is shown for shape only (do not parse against this
doc). It references regions **by name + offset**, never by raw GPA. Region names are
workload-defined strings declared in the WorkloadImage manifest's `regions` list
(reference-workload API.md §4); the demo's canonical names are `wram` and
`framebuffer`:

```yaml
# reference-workload feature-maps/demo-game.yaml (canonical: reference-workload API.md §1)
schema_version: 1
kind: feature-map
meta: { name: demo-game, workload: refwork-demo, version: 3 }
regions:                           # sizes the map was authored against; consumers
  - { name: wram,        size: 131072 }   # verify published size >= declared
  - { name: framebuffer, size: 229376 }
features:
  - name: progress_flags
    region: wram                   # must exist in `regions` and in the manifest
    offset: 0x0DBE                 # byte offset within the region (extent-spanning OK)
    type: u16le
    semantics: progress_flag
    stability: stable
```

Per exploration step the hypervisor's capture engine resolves each referenced region
once via the manifest and bulk-reads the compiled `(region, layout_version, offset,
len)` extraction ranges (`read_region`), returning the packed feature bytes +
framebuffer with its Run/TakeSnapshot responses (the orchestrator compiles the feature
map into that extraction list at experiment start; transport defined by the
hypervisor's API, not here). On `layout_version` mismatch between the extraction range
and the manifest entry the read fails loudly and the step is marked invalid — never
score garbage. The scorer never reads guest memory or subscribes to guest events
itself: `detsdk.stats` bytes (e.g. `beacon_counts`, API.md §4.3) reach it only if the
feature map references that region, via the same compiled extraction ranges inside
`ScoreBatch.feature_bytes`; first-hit `Beacon`/`Reachable` events go to the
**orchestrator** (`RunResponse.sdk_event` / `StreamGuestEvents`), which relays them
post-commit as observatory events (`reachability-hit` etc. — orchestrator API.md §6).

## 4. Fault decisions round-trip through the input log

- **Exploration (record)**: `input-synthesizer` attaches an optional **fault plan** to
  a burst: an ordered list of `(match, decision)` rules (match by `name` glob and/or
  occurrence index). The hypervisor wraps it in a `FaultPlan` impl. When the guest's
  `inject_point` detcall arrives, the responder computes the decision, **appends
  `(icount, pio_answer{port=0xD384, value})` to the input log**, and returns it.
- **Replay**: the replay `FaultPlan` is the input log itself: when the same `IN
  0xD384` exit occurs (same icount, because everything before it was identical), the
  hypervisor answers with the logged value. No synthesizer, no plan, bit-identical.
- **Invariant**: the decision is *only* ever a function of logged data. A plan that
  consulted wall time or host randomness at decision time would still be replayable
  (the log stores the outcome, not the reason), but for debuggability plans should be
  pure functions of `(iseq, name, occurrence)`.

## 5. Sequence diagram: boot → register → explore step

```
 hypervisor              agent (PID1)            workload+SDK           scorer/orchestrator
     │                       │                        │                        │
     │   ── VM power-on ──►  │                        │                        │
     │                       │ alloc 2MiB hugepage,   │                        │
     │                       │ write ChannelHeader    │                        │
     │ ◄─ OUT D374/D378 ──── │ (GPA latch)            │                        │
     │ ◄─ OUT D37C (init) ── │                        │                        │
     │ attach+validate       │                        │                        │
     │ ── IN D37C = 0 ─────► │                        │                        │
     │ ◄─ Hello (ringA) +    │                        │                        │
     │    doorbell ────────  │                        │                        │
     │                       │ autostart unit 0 (boot │                        │
     │                       │ manifest; NO ring-C    │                        │
     │                       │ command) fork+exec,    │                        │
     │                       │ pass CHANNEL_FD ─────► │ init(): map channel,   │
     │                       │                        │ iopl(3), map pv-pad,   │
     │                       │                        │ stats region           │
     │                       │ Hello ──(ctl fd3)────► │   (CtlMsg set owned by │
     │                       │ ◄───────── HelloAck ── │    refwork API.md §3)  │
     │                       │ LoadGame{/dev/vdb} ──► │ mmap ROM, build core   │
     │                       │ ◄──────── GameLoaded ─ │                        │
     │                       │ ◄─ RegisterRegion ──── │ mlock+prefault wram/fb │
     │                       │ pagemap GVA→GPA,       │                        │
     │                       │ manifest write         │                        │
     │ ◄─ RegionRegister     │ (seqlock gen 2k)       │                        │
     │    (ringA)+doorbell ─ │                        │                        │
     │                       │ ◄─ Ready{frame:0}(ctl) │                        │
     │                       │ Start{} ─(ctl fd3)───► │ free-running frame loop│
     │ ◄─ Ready (ringA) +    │ Start ok + expected    │                        │
     │    doorbell ────────  │ regions live → READY   │  [bootstrap: TakeSnapshot →
     │ read manifest:        │ point (ARCH §4.1/§4.2) │   root node, owned by  │
     │ "wram" → [{gpa,len}]  │                        │   the orchestrator]    │
     │                       │                        │                        │
     ├─────────────── exploration step (×K branches) ────────────────────────────┤
     │ restore snapshot(N)   │                        │                        │
     │ land PAD_SET → pv-pad │                        │ poll_input: one latch  │
     │ latch @ at_frame      │                        │ read per frame         │
     │ (logged)              │                        │ frame_mark() per frame │
     │ run T guest-seconds   │                        │                        │
     │ pause @ icount        │                        │                        │
     │ drain ringA/W (cons   │                        │                        │
     │ bump logged DEV_EVENT)│                        │                        │
     │ read_region(wram,fb) ─┼────────────────────────┼──────────► features ──►│ score,
     │ stream GuestEvents ───┼────────────────────────┼──────────────────────► │ novelty,
     │ dirty pages → store   │                        │                        │ commit/discard
```

## 6. Sequence diagram: `inject_point` round trip

```
 workload+SDK                          hypervisor (PIO handler + logs)      input log
     │                                          │                              │
     │ inject_point("disk_read")                │                              │
     │  iseq = next_inject_seq()    // e.g. 7   │                              │
     │  ringW.push(InjectQuery{7, name_id})     │                              │
     │  release-store ringW prod                │                              │
     │  OUT 0xD384, eax=7  ── VM exit ────────► │ drain ringW                  │
     │                                          │  (cons bump) ──────────────► │ (icount, cons=…)
     │                                          │ match iseq 7 →               │
     │                                          │ FaultPlan.decide(7,          │
     │                                          │   "disk_read")               │
     │                                          │  = Platform{kind=2,arg=512}  │
     │                                          │ pack: kind | arg<<8          │
     │                                          │  = 2 | 512<<8 = 0x0002_0002  │
     │  IN eax, 0xD384  ◄── resume ───────────  │ answer ────────────────────► │ (icount, pio 0xD384=v)
     │  decode → ShortCount(512)                │                              │
     │  → emulate short read                    │                              │
     │                                          │                              │
     ├────────────────────────── REPLAY ──────────────────────────────────────┤
     │ identical execution → same OUT/IN        │ answers IN from log entry    │
     │ at same icount                           │ (no FaultPlan consulted)     │
```

(Packed value for `Platform{kind=2, arg=512}`: `2 | (512 << 8)` = `0x0002_0002`.)

## 7. observatory & replay-renderer touchpoints (indirect)

- `observatory` never touches this repo directly: guest events reach it as
  orchestrator-relayed observatory events (`assertion-violated`, `reachability-hit` —
  the orchestrator subscribes to `StreamGuestEvents`/`RunResponse.sdk_event` and
  relays post-commit, orchestrator API.md §6); coverage intensities come from the
  decoded feature values the orchestrator publishes in `node-added`, not from any
  direct `detsdk.stats` read.
- `replay-renderer` consumes nothing from this repo directly; its determinism check is
  end-to-end (re-execute the input log, compare state hashes), which transitively
  verifies every channel interaction because all host-side channel writes are in the
  log. For video rendering it uses the hypervisor's `RunWithFrameCapture`
  server-streaming RPC (determinism-hypervisor API.md §2.7), whose capture path
  resolves the `framebuffer` region via this repo's manifest — there is no per-frame
  `ReadGuestMemory` loop.

## 8. Compatibility matrix

| Consumer | Depends on | Breaking-change gate |
|---|---|---|
| determinism-hypervisor | `detguest-host` crate API, detcall ABI, channel layout, record framing | `proto_version` |
| reference-workload | `detguest-sdk` crate API, pv-pad latch bit semantics (owns them; latch device owned by determinism-hypervisor), region names (owns them, declared in its WorkloadImage manifest), `boot.toml` format (API.md §7 — refwork bakes the file) + the agent's control-protocol driving (ARCHITECTURE.md §4.2; CtlMsg wire owned by refwork API.md §3) | crate semver, `boot_toml_version` |
| state-scorer | region names + `layout_version`s, `detsdk.stats` layout, feature-map schema (owned by reference-workload) | `layout_version`, `stats_version` |
| input-synthesizer | fault-plan rule schema (`name`, occurrence, `FaultDecision` kinds) | `FaultDecision` packing (proto_version) |
| control-plane | `GuestEvent` payload framing (API.md §3.2), carried by determinism-hypervisor's `GuestEvent` proto (owned in its API.md) | proto_version |
