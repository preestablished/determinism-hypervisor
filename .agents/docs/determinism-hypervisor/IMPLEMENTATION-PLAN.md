# determinism-hypervisor — Implementation Plan

Ordered milestones; each has acceptance criteria that are **executable** (a test or a
measured number), per the verification discipline: "it builds" is never "it works".
All milestones run on the Intel box; CI determinism jobs run on a KVM-capable
self-hosted runner there (label `kvm-intel`).

References: ARCHITECTURE.md (mechanisms), API.md (schemas). The platform-level
Milestone 1 from MAP.md ("fork a guest 1000× and verify bit-identical re-execution")
is **M7** below.

---

## Milestones

### M0 — Workspace + KVM smoke
Workspace scaffolding (crate layout per ARCHITECTURE §1), `dh-vmm` boots a 20-line
real-mode→long-mode stub that writes to debug-serial and HLTs.
**Accept:**
- `cargo test` green on the runner; `dh-cli boot tests/nanokernel/hello.elf` prints
  the expected bytes and exits.
- Preflight checker (`dh-workerd --preflight`) implemented and passing on the box
  (ARCH §7.4 host config **and** the full §2.1 required-capability table, including
  `KVM_CAP_X86_USER_SPACE_MSR` with `KVM_MSR_EXIT_REASON_FILTER`), failing loudly on
  a stock kernel config or a missing capability.

### M1 — Nanokernel guest + pv devices + boot from image
ELF boot path, full MMIO map + PIO detcall handler, pv-clock/pad/entropy/blk
(+overlay)/serial device models plus the detchannel host side (`detguest-host`
attach/drain — guest-sdk owns the channel spec), nanokernel extended to exercise each
device. Read-only base image + CoW overlay working.
**Accept:**
- Nanokernel test program: reads clock, draws entropy, reads pad latch, writes/reads
  blk sectors (overlay verified — base image file byte-unchanged, mtime-checked),
  initializes a minimal detchannel page and sends ring-W records via the doorbell
  detcall; host asserts every value and logs every channel mutation as DEV_EVENT.
- `MachineConfig` plumbed end to end; CPUID determinism mask applied and dumped by
  `dh-cli cpuid-diff` for review.

### M2 — detclock: counting + exact landing
`dh-detclock` perf counter (guest-only, pinned), PMI→immediate-exit kick, boundary
engine with skid margin + single-step refinement, REP rule, counting-semantics
empirics.
**Accept:**
- `counting_semantics` test: single-step a known 1,000-instruction nanokernel sequence
  (including REP MOVS, CPUID, MMIO exits); counter delta exactly 1,000; REP retires
  as 1.
- Landing test: for 10,000 random targets N in a 100M-instruction nanokernel loop,
  stop at exactly N (`icount == N`, RIP at instruction start) with **zero overshoots**.
- Skid histogram exported; measured max skid on the box < skid_margin/2 (else raise
  margin and re-baseline).

### M3 — Virtual time, deterministic injection, run control
Agenda/scheduler (ARCH §3.3), pv-clock timers via §3.4 injection rule, `Run(until …)`
semantics including goal polling, next_sdk_event, and frame_budget stops,
Pause→epoch-boundary roll-forward.
**Accept:**
- Nanokernel arms a timer every 1ms-vns for 10s-vns; delivered icounts are identical
  across 100 repeated runs (exact-match list).
- IF=0 deferral test: timer lands while guest has interrupts masked; delivery defers
  identically across runs.
- First **determinism regression test** lands in CI: run nanokernel 1e9 instructions
  twice from cold boot with fixed seed, final state hash equal. This job becomes
  required-for-merge from M3 onward.
- Guest-TSC alignment mechanism benchmarked: per-entry `KVM_SET_MSRS{IA32_TSC}` value
  writes vs the `KVM_VCPU_TSC_CTRL` offset attribute (ARCH §4 defense 4 caveat) — pick
  the cheaper/safer one with measured numbers before M4 freezes restore behavior.

### M4 — Snapshot / restore / fork + snapshot-store integration
Dirty ring harvest, DHSNAP codec (golden-bytes tests), XSAVE canonicalization, state
hash chain, TakeSnapshot/RestoreSnapshot against a live snapshot-store, tier-A CoW
fork, tier-B mmap restore.
**Accept:**
- Roundtrip: boot → run 1e8 → snapshot → destroy → restore → run 1e8 more → hash H1.
  Versus: boot → run 2e8 → hash H2. **H1 == H2** (snapshot transparency — the critical
  property; an instruction-count or device-state leak shows up here).
- Fork transparency: same test with a tier-A fork in the middle; and parent frozen →
  child diverges → parent's second child re-run matches first child given same inputs.
- Dirty-ring-full forced (smallest legal ring — 1024 on the lab box; the kernel's 64+512 PML reserved floor EINVALs 512) — snapshot refs unchanged vs large ring.
- ENTR golden test: snapshot → restore reproduces the **next N (= 1024) entropy
  draws bit-identically** (exercises the `{seed, stream, word_pos}` round trip,
  API.md §4).
- Perf gates (p50 on the box, 128 MiB demo guest — MAP.md canonical figure):
  fork < 10 ms, incremental snapshot ≤ 8k dirty pages < 150 ms, tier-B warm
  restore < 450 ms. ACCEPTED-AS-MEASURED (bead 8ot decision, 2026-06-12;
  ledger #20): the box's ext4 LV sustains ~350 MB/s durable, so the original
  numbers (snapshot < 15 ms, restore < 150 ms — they imply > 2 GB/s durable)
  are retained only as improvement TARGETS; correctness outranks speed.
  Measured at acceptance: fork p50 326 µs, snapshot p50 103 ms, restore p50
  307 ms. NOTE the M7 tension: the exploration-step budget (≤ 100 ms p50
  end-to-end) cannot contain a snapshot or restore at these accepted numbers —
  the storage-improvement work must land before that gate, or it needs the
  same decision.

### M5 — Input log (DHILOG) + replay
`dh-inputlog` full codec (golden bytes + `cargo fuzz` target), recording during runs,
PAD_SET/DEV_EVENT/NET_RX landing, AUX records, sealing, replay path, log concatenation.
**Accept:**
- Record/replay: scripted 60s-vns pad sequence on nanokernel's pad-echo program;
  replay from snapshot reproduces end_state_hash and every EPOCH_HASH. Repeat 100×.
- Fuzz: 24h `cargo fuzz` on the parser, no panics/OOM (then 1h job kept in CI nightly).
- Golden-bytes fixtures for DHILOG v1.0 and DHSNAP v1.0 checked in; byte-identical
  re-serialization asserted.
- at_frame scheduling (absolute FRAME_COUNTER basis, API.md §2.3) and frame_budget
  stops verified against the FRAME_MARK table (nanokernel emits fake frames,
  including across a snapshot/restore so the absolute counter carries over).

### M6 — Worker daemon (gRPC) + introspection + capture engine
`dh-workerd`: slot manager, leases, full proto surface (API.md §2), ReadGuestMemory /
GetFramebuffer / StreamGuestEvents, /healthz + metrics, WatchSlots, and the **capture
engine** (ARCH §6.10: `CaptureSpec` on Run/TakeSnapshot, region-manifest resolution,
`feature_bytes` packing, lz4 framebuffer). The capture engine lands here and is
exercised against the real guest-sdk region manifest in **Phase 3** (alongside
guest-sdk Ms4 — Phase 3's "RAM/framebuffer host-readable" gate has no other producer).
**Accept:**
- Integration test drives the whole API over UDS: restore→inject→run→snapshot(with
  CaptureSpec)→destroy, 64 slots concurrently, no cross-slot interference
  (per-slot hashes match single-slot baselines — catches PMU counter collisions and
  core-pinning bugs).
- Capture-neutrality test (ARCH §6.10 C5): identical child refs and epoch hashes for
  capture vs no-capture runs of the same (snapshot, inputs); `layout_version`
  mismatch fails `FAILED_PRECONDITION`.
- `grpcurl` smoke documented; metrics include every ARCH §9 series.

### M7 — **Platform Milestone 1: fork 1000× + verified re-execution**
End-to-end: boot guest → root snapshot → 1000 forks (batched across slots), each runs
a distinct 1-guest-second random pad burst (seeded), TakeSnapshot each, then
VerifyReplay each (snapshot, log) pair.
**Accept (MAP.md build-order milestone):**
- 1000/1000 VerifyReplay return VerifyDone with matching end_state_hash; zero
  Divergence.
- Determinism cross-check: 10 of the 1000 jobs re-run from the *root* on a different
  slot reproduce identical child snapshot refs (content-addressed ⇒ bit-identical).
- Throughput: sustained ≥ N_slots × 1 job/s (within the §10 per-job budget) for
  ≥ 30 min under simultaneous host load (a `stress-ng` housekeeping-core job) — exits,
  PMI, and hashes unaffected by load (hashes are the assertion).

### M8 — Verification mode hardening + bisection
`dh-verify` divergence bisection (INTEGRATION §4), suspected-cause decoder, VerifyReplay
streaming, FAULTED_S quarantine.
**Accept:**
- Fault-injection tests *deliberately* break determinism five ways (host-time read in
  a test device, skipped entropy log, off-by-one injection icount, un-canonicalized
  XSAVE, stray TSC write) — bisection localizes each to ≤ 1024 instructions and the
  right suspected_cause, within 10× the segment's original runtime.

### M9 — Minimal-Linux guest path (bzImage)
Only after M7: the bzImage boot protocol, lapic-stub coverage for early boot, the
deterministic cmdline baseline (`init=/init` — the initramfs shim that execs
`/sbin/detguest-agent`; image layout owned by guest-sdk), guest-sdk handshake from the
Linux userspace agent.
**Accept:** boot-to-READY-beacon deterministic (two boots, equal hashes at beacon);
all M4/M5 regression tests pass on the Linux guest too. (The demo can ship on the
unikernel path if this slips; `reference-workload` decides what it needs.)

---

## Testing strategy

| Layer | What | Where it runs |
|---|---|---|
| Unit | DHILOG/DHSNAP codecs (golden bytes, fuzz), agenda math, vns rational math, XSAVE canonicalization, overlay cluster logic | any host (no KVM) — these must build/test on aarch64 too so Spark-side devs can touch shared code |
| KVM integration | boundary landing, counting semantics, snapshot transparency, device models against nanokernel | `kvm-intel` runner |
| **Determinism regression (CI-required)** | (a) run-twice-compare-hash on every PR; (b) nightly: M7-style 100-fork verify; (c) record/replay corpus — a checked-in set of (root image, DHILOG) pairs with expected hashes, re-verified nightly so *any* behavioral drift (code, kernel, microcode) is caught with a named culprit | `kvm-intel` runner |
| Perf gates | criterion benches for fork/snapshot/restore/landing; regression > 20% fails the nightly | `kvm-intel` runner, quiesced |
| Chaos | host load, tiny dirty rings, forced PMI storms, snapshot-store latency injection | nightly |

Host-environment pinning: the runner's kernel + microcode versions are recorded in
repo (`ci/determinism-class.lock`); the nightly fails if the host drifted from the
lock without a deliberate bump (a bump requires re-baselining the record/replay corpus
— this is the *procedure* for absorbing host changes, not an incident).

---

## Key risks & mitigations

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | **PMI skid** larger than margin → overshoot past target icount | Missed boundary = wrong landing = divergence | Conservative default margin (8192); measured max-skid histogram with alert at margin/2; overshoot is a loud `DATA_LOSS`, never silently absorbed; margin is config, raising it costs only landing latency |
| R2 | **Retired-instruction counter semantics drift or failure** — INST_RETIRED's interrupt-retirement behavior is an empirically validated per-class *assumption* (ARCH §3.1), not an architectural guarantee; microcode/kernel updates can change counting around interrupts, REP, MMIO-exiting instructions; PMU count errata exist | Every stored icount becomes unreplayable on the new host; worst case, INST_RETIRED is unusable on the lab CPU | Counting-semantics empirics test in CI; determinism-class lock + re-baseline procedure; icounts only compared within a recorded determinism class; corpus catches drift the day it lands. **Documented fallback counter:** switch `dh-detclock` to **retired conditional branches** (`BR_INST_RETIRED.COND`/`.NEAR_TAKEN`) with the `(count, RIP, RCX)` boundary tuple — the known-good alternative from record/replay practice; the boundary engine already carries RIP and RCX, so the swap is contained in `dh-detclock` + a determinism-class bump |
| R3 | **Stray RDTSC/RDTSCP** between exits (guest contract violation or missed kernel path) | Nondeterministic value reaches guest state | Layered defenses (ARCH §4): pv-clocksource-only guests, CPUID mask, CR4.TSD, TSC re-write at entry; verification mode catches + names it (suspected_cause). Future hardening: tiny host-kernel module enabling the VMX RDTSC-exiting execution control (deliberately deferred — out-of-tree kernel code is its own risk) |
| R4 | **RDRAND/RDSEED ignore CPUID masking** (no trap available from userspace KVM) | Hardware entropy reaches guest | Curated images audited (objdump scan for the opcodes in CI of guest-sdk/reference-workload); verification backstop; same future kernel-module option as R3 |
| R5 | **lAPIC/timer nondeterminism** — any in-kernel irqchip or guest-armed hardware timer ticking on host time | Interrupt timing varies run to run | No in-kernel irqchip at all (ARCH §2.2); all interrupts injected at boundaries; lapic-stub is plain Rust state; KVM PIT/kvmclock never created; ARAT/TSC_DEADLINE masked |
| R6 | **MSR surprises** (guest touches an MSR whose value is host-derived: APERF/MPERF, PLATFORM_INFO, SMI count…) | Host state leaks into guest | Default-deny MSR filter: unknown RDMSR → deterministic emulated value or guest #GP, never the hardware value; capture list is closed-form (ARCH §8.1) |
| R7 | **XSAVE area byte-instability** for logically equal state | False divergence / unstable snapshot refs | Canonicalization (zero components with clear XSTATE_BV bits) on both snapshot and hash paths; M4 fault-injection test covers it |
| R8 | **Dirty-page tracking misses** (PML edge cases, ring overflow handling, huge pages) | Snapshot misses a changed page → silent corruption that *looks* like nondeterminism | THP off (4 KiB exact); ring-full chaos test; M4 roundtrip test catches misses by construction (H1 ≠ H2); periodic full-memory hash audit mode (`--paranoid-hash`) for soak runs |
| R9 | **CoW fork aliasing bugs** (frozen parent written; child sharing more than memory) | Cross-branch contamination | `F_SEAL_FUTURE_WRITE` on frozen parents' memfds blocks *new* writable mappings (kernel-enforced for that scope only — `F_SEAL_WRITE` is unavailable while the parent's KVM mapping exists, ARCH §8.4); the **software-enforced** `Frozen` slot-state machine is the guard against the parent's own existing mapping writing; M4 fork-transparency tests; per-slot KVM fds (no shared kernel state) |
| R10 | **Single-step refinement perturbs guest state** (TF flag visibility, DR ownership) | Guest observes the landing machinery | KVM_GUESTDBG keeps TF out of guest-visible RFLAGS on exits; guests don't use DRs (contract; DR writes fault); determinism tests run with landing-heavy schedules to prove invisibility |
| R11 | **Performance: landing cost blows the <1s budget** on injection-dense workloads | Throughput, not correctness | Per-frame inputs ⇒ ~60 landings/guest-s; measured budget in ARCH §10 has 2× headroom; HW-breakpoint fast path documented as the next optimization; FINAL_ONLY hashing mode |
| R12 | **snapshot-store coupling** (page-channel throughput or GC interplay) | Snapshot latency, lost pages | Fast path co-designed with snapshot-store (its README fast-path section); refs returned only after durability; pages immutable while referenced — joint M4/M7 integration tests run against the real store, not a mock |
| R13 | **Multi-vCPU pressure arrives early** (a workload needs SMP before v2) | Scope explosion | Explicit non-goal + documented path (ARCH §11); guests are built UP; pushing back is a product decision with a design doc ready, not an emergency hack |

---

## Definition of done (service v1)

M0–M8 accepted (M9 optional per `reference-workload` needs); CI required-jobs green
including the determinism regression suite; ARCH §10 perf table met at p50 on the
Intel box; INTEGRATION.md flows exercised against real `snapshot-store` and a stub
orchestrator script; zero open `DATA_LOSS`-class bugs.

Out of v1 scope, scheduled later: `RunWithFrameCapture` (API.md §2.7) is required by
replay-renderer and lands with the **Phase 7** proof-pipeline work (its
capture-neutrality test reuses the M6 harness); the control-plane image-blob fetch
path (ARCH §9) lands with the Phase 6 deployment track.
