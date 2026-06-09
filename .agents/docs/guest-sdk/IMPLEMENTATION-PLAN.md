# guest-sdk Implementation Plan

Ordered milestones. Each has acceptance criteria a CI job (or the in-VM harness on the
Intel box) can check mechanically. Determinism failures anywhere are P0 per MAP.md.

## Milestone 0 — `detguest-wire`: formats + golden tests

Build the shared format crate first; everything else is a client of it.

**Work**
- `ChannelHeader`, ring descriptors, drop counters with the exact offsets in
  ARCHITECTURE.md §2; `const` offset assertions (`static_assert`-style) so layout
  drift fails compilation.
- `RecordHeader` + all EventKind/CommandKind/WorkloadCtrlKind payloads (API.md §3,
  incl. `FrameMark` and `Ready`), encode/decode with no-wrap + `Pad` framing rules.
- `RegionManifest` read/write incl. seqlock helpers (API.md §4).
- detcall port constants + `FaultDecision` pack/unpack (API.md §1.4, §5).
- `#![no_std]` + `alloc` feature; zero unsafe outside documented ring-pointer code.

**Acceptance**
- Golden vectors: one checked-in binary fixture per record kind (`tests/golden/*.bin`)
  with byte-exact encode and decode assertions, including: truncated AssertViolation
  details, `Pad` at ring tail, max-size LogLine, dead manifest entry, packed
  `FaultDecision` values `{Proceed, Platform{2,512}=0x00020002, Workload{200,0xFFFFFF}}`.
- Round-trip property test (`proptest`): decode(encode(x)) == x for all kinds;
  decoder never panics on arbitrary bytes (fuzz target `cargo fuzz run decode_record`,
  30 min clean).
- `cargo test --no-default-features` passes (no_std build).

## Milestone 1 — `detguest-host` over a mock `GuestMem`

**Work**
- `GuestMem` trait + `Vec<u8>`-backed mock.
- `Channel::attach` validation paths; `drain_events` (both rings, ordering, partial
  records mid-write tolerated by stopping at the last complete record); `push_command`
  / `push_workload_ctrl`; `read_manifest` with seqlock retry; `read_region` extent walk;
  `ChannelWriteSink` plumbed through every mutation.
- `InjectResponder` + `FaultPlan` trait with two impls: `TableFaultPlan` (tests) and
  `LogFaultPlan` skeleton (replay; final wiring lands in determinism-hypervisor).
- Intern-table maintenance from `NameIntern` events.

**Acceptance**
- A pure-host loopback test: a guest-side simulator (using `detguest-wire` producer
  code against the same mock memory) produces 10⁵ mixed events incl. wrap, pad, drops,
  and a registration; `drain_events` recovers exactly the non-dropped sequence; drop
  counters match the simulator's bookkeeping; every host mutation appeared exactly once
  in the recorded `ChannelWriteSink` trace.
- `read_region` correctly stitches a 3-extent region across a discontiguous mock layout.

## Milestone 2 — `detguest-agent` boots as PID 1; channel up; logs flow

**This milestone is self-contained: the agent boots under this repo's own minimal KVM
test harness and kernel build, independent of every `determinism-hypervisor`
milestone** (the hypervisor only gains a Linux-guest boot path at its M9; waiting for
it would stall this repo for two phases — see Dependency notes).

**Work**
- Static musl binary; init mounts (§4 ARCHITECTURE.md); hugetlbfs channel alloc;
  pagemap self-translation; CHANNEL_INIT detcall; Hello; `boot.toml` parsing + boot
  fault path (API.md §7); boot-manifest autostart + `Ready` emission (the
  deterministic READY point, ARCHITECTURE.md §4.1); ring-C poll loop; Shutdown.
- `image/`: minimal kernel config (the determinism set: `CONFIG_COMPACTION=n`,
  `CONFIG_MIGRATION=n`, `CONFIG_KSM=n`, no THP, no swap, single CPU; cmdline flags
  such as `norandmaps` come from the canonical kernel cmdline the hypervisor forces —
  determinism-hypervisor ARCHITECTURE.md §2.3 owns it) +
  initramfs builder script. Init path is the initramfs `/init` shim exec'ing
  `/sbin/detguest-agent` — the image's only init binary (no `dh-init` exists anywhere).
  Coordinate the config file with `reference-workload` (which bakes the emulator into
  this image).
- Test harness: this repo's own minimal KVM runner in `tests/vm/` — own kernel build,
  PIO handler + `GuestMem` over the memslot, and a trivial pv-pad MMIO latch stub (for
  M3's `poll_input`/`frame_mark` tests). It mirrors `determinism-hypervisor`'s KVM
  setup path but does not depend on that repo shipping anything.

**Acceptance** (in-VM, Intel box)
- VM boots to agent in < 1 s guest time; host sees IDENT, INIT_GO status 0, Hello with
  `proto_version 1`.
- With a trivial autostart workload configured (empty expected-regions list): `Ready`
  arrives, and its doorbell exit lands at a **bit-identical icount across 10
  consecutive boots** of the same image (the READY-point reproducibility contract,
  ARCHITECTURE.md §4.1, measured by the harness's retired-instruction counter).
- `Shutdown{graceful}` powers off the VM; `WorkloadExited` semantics verified with a
  trivial baked-in workload that prints to stdout (host receives `LogLine` events with
  correct stream/level framing).

## Milestone 3 — `detguest-sdk` end-to-end events from a real workload

**Work**
- SDK `init()` (channel fd inherit, `iopl(3)`, pv-pad MMIO window mapping, stats
  region auto-registration), intern table, `assert_always` (+ repeat limit),
  `expect_reachable`/`declare_reachable`, `coverage_beacon` (counter array + first-hit
  event), `log_line`, `poll_input` (pv-pad latch read wrapper), `frame_mark`
  (FrameMark event + FRAME_COUNTER write ordering), `quiesce_check` (incl. ring-I
  control-record consumption), critical-event doorbell-retry, droppable-drop policy.
- Agent IPC socket (`SOCK_SEQPACKET`) + `RegisterRegion` request handling end-to-end:
  mlock/prefault in SDK, pagemap translation + extent coalescing in agent, manifest
  seqlock write, `RegionRegister` event.
- A `testload` binary (lives in `tests/vm/`) exercising every API call.

**Acceptance** (in-VM)
- `testload` run produces the exact expected event stream (golden event-stream hash):
  interns, one AssertViolation with details, first-hit Reachable and Beacon, LogLines,
  WorkloadExited.
- Overflow test: `testload --spam-logs` overflows ring W; host-observed drop counters
  equal producer-side expected counts; zero critical events lost; the doorbell-retry
  path is exercised (`--spam-asserts`) without deadlock.
- `poll_input`/`frame_mark`: the harness's pv-pad latch stub lands 1000 scheduled pad
  values (one per simulated frame); `testload` reads the latch once per frame, calls
  `frame_mark()`, and echoes the observed sequence via LogLine digests; order and
  values exact, and the harness sees one `FrameMark` record + one FRAME_COUNTER write
  per frame in the contract order (record before MMIO write).

## Milestone 4 — Memory publication usable by the platform ⭐

**The named milestone: emulator RAM region readable from host and stable across
snapshot/restore.**

**Work**
- Integrate with `reference-workload`'s emulator: hugetlbfs arena, register
  `wram` / `framebuffer` (plus the optional `vram` — region names are
  reference-workload's, declared in its WorkloadImage manifest; INTEGRATION.md §2).
- Agent control-protocol leg against the real harness: `Hello → LoadGame → Start`
  driving with `Ready` gated on Start-success + expected regions (ARCHITECTURE.md
  §4.2; CtlMsg wire owned by reference-workload API.md §3), including the
  fault-before-Ready path.
- `ReverifyRegions` command path.
- Hand `determinism-hypervisor` the `read_region` + manifest APIs and the per-VM
  channel-GPA metadata hook for post-restore re-attach.

**Acceptance** (in-VM, with determinism-hypervisor's snapshot/fork from build-order
step 1)
- Host reads `wram` while the emulator runs a known ROM; bytes at a known
  offset match the emulator's own debug dump at the same frame (FRAME_MARK-gated pause).
- Snapshot the VM, restore it 100×: manifest identical (byte compare), `wram`
  extents identical, region contents at restore identical to at-snapshot contents
  (hash compare). Fork 100 children, run each 60 guest-frames with different inputs:
  every child's `read_region` works without any guest round trip.
- `ReverifyRegions` after 10 minutes of churn workload reports zero moved extents
  (validates the kernel-config pinning guarantees).

## Milestone 5 — `inject_point` + input-log round trip + determinism proof

**Work**
- SDK `inject_point` (event + OUT/IN detcall); host `InjectResponder` wired to the PIO
  handler; `(icount, pio_answer)` log records; `LogFaultPlan` replay path (with the
  hypervisor team).
- The repo's flagship CI test, **`determinism_replay`**: run `testload` (which calls
  every SDK API incl. 50 inject points under a `TableFaultPlan`) for N guest-seconds;
  record the input log; replay from the root snapshot; compare (a) final guest RAM
  hash, (b) the complete drained event stream byte-for-byte, (c) drop counters,
  (d) all inject decisions observed by the workload (echoed via LogLine digest).

**Acceptance**
- `determinism_replay` passes 1000 consecutive iterations with varied fault plans and
  input bursts (seeded, logged). Any mismatch fails CI — this test is the repo's
  mandatory determinism regression gate per MAP.md conventions.
- Replay answers inject queries from the log with the synthesizer absent.

## Milestone 6 — Quiesce, hardening, perf, docs-as-built

**Work**
- Cooperative + forced quiesce paths; virtual-time bounded wait on the host side.
- Perf pass: `coverage_beacon` < 10 ns hot path; `assert_always(true)` < 15 ns;
  ring W sustained ≥ 200 MB/s drain at pause boundaries; `inject_point` round trip
  < 20 µs.
- `cargo deny` / unsafe audit; `#![forbid(unsafe_code)]` everywhere except
  `wire::ring`, `sdk::pio`, `sdk::regions`, `agent::translate`.
- Update these docs to as-built; add `/healthz`-equivalent: the agent's Hello carries
  capability bits, and the host crate exposes Prometheus-ready counters (drained
  events, drops observed, inject answered, manifest generation) for the hypervisor to
  export per MAP.md conventions (the in-guest side itself exposes no HTTP — the
  hypervisor is its health proxy; note this deviation explicitly).

**Acceptance**
- Quiesce: COOP round trip lands the snapshot at a frame boundary (verified by frame
  counter parity in `wram`); FORCED works on a non-SDK workload; timeout path
  falls back to instruction-precise pause without error.
- Criterion benches checked in with thresholds; perf CI job on the Intel box.

## Testing strategy (cross-cutting)

| Layer | Test | Where it runs |
|---|---|---|
| wire | golden vectors (byte-exact), proptest round-trip, decoder fuzz | any host, CI |
| host | loopback simulator over mock GuestMem; ChannelWriteSink trace audit | any host, CI |
| agent/SDK | in-VM harness (`tests/vm/`): boot, events, inputs, overflow, regions | Intel box (KVM) |
| platform | `determinism_replay` (bit-identical re-execution incl. SDK + injects) | Intel box |
| pinning | snapshot/restore ×100 + churn + ReverifyRegions | Intel box |
| perf | criterion benches with thresholds | Intel box |

CI tiering: wire+host tests run everywhere (including aarch64 — the DGX Spark can run
them; nothing in those crates is x86-specific). In-VM tiers are gated to the Intel
runner. The detcall ABI is x86-only by design; `detguest-wire` keeps port constants
behind a module so a future aarch64 guest ABI slot exists but is out of scope.

## Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Guest kernel migrates an mlocked page (compaction/CMA path missed by config) | Host reads stale GPA → garbage features, P0 | Kernel config set in M2 (`COMPACTION=n`, `MIGRATION=n`, `KSM=n`, no THP, no swap); `ReverifyRegions` in CI churn test; manifest `generation` lets host detect any republish |
| `/proc/pid/pagemap` PFN visibility requires privilege | Registration fails | Agent is root with CAP_SYS_ADMIN in the minimal image; assert at agent startup, fail loud in Hello capabilities |
| Ring index torn reads / missing barriers | Corrupt records, heisenbugs | SPSC only; acquire/release discipline encapsulated in `wire::ring` (the single unsafe module); loopback test runs under `miri` for the index logic and under `loom` for the producer/consumer interleavings |
| Host drains at unlogged points (dev shortcut) | Replay divergence — worst-class bug | `ChannelWriteSink` is a **required** parameter on every mutating call; no mutate-without-sink API exists; `determinism_replay` in CI catches violations |
| KVM intercepts or mishandles the PIO range | detcalls never reach the VMM | Ports `0xD370–0xD39F` are outside all emulated-device ranges the hypervisor claims; M2 harness asserts IDENT works before anything else; fallback documented (MMIO page) but not built |
| Ring W overflow drops Beacon discovery events under spam | Scorer misses new-coverage signal | Beacon counts live in the stats region (not the ring); only first-hit events ride the ring, bounded at 65,536 total per process |
| `inject_point` exit cost too high for hot call sites | Slows exploration throughput | Documented placement guidance (I/O boundaries); perf budget in M6; future batched "fault prefetch" reserved as proto v2 idea, not built now |
| Name-intern table growth / collisions across workload restarts within one VM | Mis-attributed events | Intern ids are per-process-lifetime; `WorkloadStarted` resets the host's per-channel table scope; table capacity 64 Ki names, hard error after |
| Manifest capacity (64 regions / 1024 extents) too small for fragmented heaps | Registration failure | Hugepage guidance keeps extents tiny for the demo; error is explicit (`TooManyExtents`); capacity doubling is a manifest_version bump, layout already leaves headroom |
| Multi-threaded workload SDK ordering assumptions | Replay divergence if vCPU count ever > 1 | v1 hard-asserts single vCPU at attach (hypervisor side); SDK spinlock documented as ordering-neutral under 1 vCPU |

## Dependency notes

- M0–M1 have zero external dependencies and unblock hypervisor-side integration early.
- **M2 is independent of `determinism-hypervisor` milestones**: the agent boots under
  this repo's own minimal KVM test harness and kernel build (M2 work item above). The
  hypervisor's M1 only provides an ELF/nanokernel boot path and its Linux-guest boot
  arrives at its M9 — neither gates M2. Any phase-plan note reading "Ms2 depends on
  hypervisor M1" is superseded by this. M4–M5 do require the hypervisor's
  snapshot/fork and input-log machinery.
- The image's init path is the initramfs `/init` shim → `/sbin/detguest-agent`; no
  `dh-init` binary exists in any image layout.
- `reference-workload` integration (M4) is the cross-repo milestone matching MAP.md
  build-order step 2: "scripted input log plays the game's first room" — that demo
  uses the pv-pad latch input path (`PAD_SET` landings + per-frame `poll_input`)
  end-to-end.
