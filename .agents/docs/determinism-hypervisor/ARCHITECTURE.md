# determinism-hypervisor — Architecture

Audience: the coding agent implementing this service. Everything here is normative
unless marked "future work". Read `README.md` first for the glossary; terms like
*icount*, *boundary*, *epoch hash*, *slot* are used without re-definition.

---

## 1. Crate layout

Single cargo workspace, one repo:

```
determinism-hypervisor/
├── Cargo.toml                 # workspace
├── proto/hypervisor.proto     # canonical gRPC schema (until control-plane exists)
├── crates/
│   ├── dh-vmm/                # core VMM library: KVM setup, guest memory, run loop,
│   │                          #   boundary engine, event scheduler. No gRPC, no I/O policy.
│   ├── dh-detclock/           # perf_event_open wrapper: guest-mode instructions-retired
│   │                          #   counter, PMI arming, rdpmc-style reads, skid handling.
│   ├── dh-devices/            # deterministic device models: MMIO bus, pv-clock, pv-pad,
│   │                          #   pv-entropy, detchannel detcall handler, virtio-blk(RO)+overlay, loopnet,
│   │                          #   framebuffer window, debug serial.
│   ├── dh-inputlog/           # DHILOG serializer/parser. Pure, no_std-compatible core,
│   │                          #   fuzzable, golden-bytes tested.
│   ├── dh-snapshot/           # capture/restore/fork; DHSNAP device-blob codec; dirty
│   │                          #   tracking; snapshot-store client (UDS gRPC + page channel).
│   ├── dh-verify/             # verification mode: epoch-hash comparison, divergence
│   │                          #   bisection, diagnostics report.
│   ├── dh-proto/              # prost/tonic generated code + thin typed wrappers.
│   └── dh-worker/             # the daemon: slot manager, gRPC server, health/metrics.
├── tools/
│   └── dh-cli/                # local debug CLI: boot, run, snapshot, replay, hexdump
│                              #   guest memory, decode DHILOG/DHSNAP.
└── tests/
    ├── nanokernel/            # tiny freestanding x86_64 test guest (no libc, ~2 KiB),
    │                          #   built by build.rs with nasm/ld; exercises every pv device.
    └── determinism/           # the regression suite (see IMPLEMENTATION-PLAN.md)
```

Dependency rules: `dh-vmm` depends on `dh-detclock`, `dh-devices`, `dh-inputlog`;
`dh-worker` depends on everything; nothing depends on `dh-worker`. `dh-inputlog` has no
deps beyond `blake3` (keeps it auditable and fuzzable).

External crates: `kvm-ioctls`, `kvm-bindings` (KVM access), `vm-memory`
(GuestMemoryMmap), `vmm-sys-util` (eventfd, ioctl helpers), `libc`, `perf-event-open-sys`
(raw `perf_event_attr` bindings; we wrap it ourselves in `dh-detclock` — do not use a
high-level perf crate, we need guest-only counting and signal-driven overflow),
`blake3`, `rand_chacha` (entropy PRNG), `lz4_flex` (framebuffer compression for the
capture engine, §6.10), `detguest-host` (guest-sdk's host-side channel library),
`tonic`/`prost`, `tracing`, `prometheus`.

---

## 2. KVM integration

### 2.1 Required capabilities (checked at startup, hard-fail if absent)

| Capability | Why |
|---|---|
| `KVM_CAP_USER_MEMORY` | guest RAM from userspace mmap |
| `KVM_CAP_SET_TSS_ADDR`, `KVM_CAP_EXT_CPUID` | basic x86 bring-up |
| `KVM_CAP_MANUAL_DIRTY_LOG_PROTECT2` | `KVM_CLEAR_DIRTY_LOG` (precise, re-armable dirty tracking) |
| `KVM_CAP_DIRTY_LOG_RING` (optional, preferred) | per-vCPU dirty ring; fall back to bitmap if absent |
| `KVM_CAP_X86_MSR_FILTER` | trap all unexpected RDMSR/WRMSR to userspace |
| `KVM_CAP_X86_USER_SPACE_MSR` (enabled with `KVM_MSR_EXIT_REASON_FILTER`) | makes filter-denied MSR accesses exit to userspace (`KVM_EXIT_X86_RDMSR`/`WRMSR`) instead of KVM injecting #GP directly — required for the deterministic MSR emulation in §2.2 |
| `KVM_CAP_GET_MSR_FEATURES`, `KVM_CAP_TSC_CONTROL` | TSC normalization on restore |
| `KVM_CAP_SET_GUEST_DEBUG` | single-step + hardware breakpoints (boundary refinement, bisection) |
| `KVM_CAP_IMMEDIATE_EXIT` | race-free kick of the vCPU from the PMI signal handler |
| `KVM_CAP_VCPU_EVENTS`, `KVM_CAP_DEBUGREGS`, `KVM_CAP_XSAVE2`, `KVM_CAP_XCRS` | complete vCPU state capture |

Forbidden: `KVM_CAP_X86_DISABLE_EXITS` (HLT/MWAIT/PAUSE in guest would burn
nondeterministic host time — we want the exits), in-kernel irqchip
(`KVM_CREATE_IRQCHIP`), in-kernel PIT (`KVM_CREATE_PIT2`), kvmclock, async page faults,
steal time, in-kernel `KVM_CAP_HYPERV_*` anything.

### 2.2 VM construction (per slot)

```rust
pub struct Slot {
    vm: VmFd,
    vcpu: VcpuFd,                     // exactly one in v1
    guest_mem: GuestMemoryMmap,       // backed by one memfd per slot (see §8)
    mmio_bus: MmioBus,                // dh-devices
    detclock: DetClock,               // dh-detclock, owns the perf fd
    sched: EventSchedule,             // pending injections, ordered by icount
    vt: VirtualTime,                  // clock_num/clock_den, derived from icount
    entropy: ChaCha20Rng,             // seeded; state is snapshotted
    log: InputLogWriter,              // open DHILOG for the current segment
    hashchain: StateHashChain,        // epoch hashes
    bounds: BoundaryState,            // last boundary (icount, rip, rcx)
    run_state: RunState,              // Paused / Running / Faulted
}
```

- **No in-kernel irqchip.** We do not call `KVM_CREATE_IRQCHIP`. With no irqchip in the
  kernel, `KVM_RUN` honors `request_interrupt_window` and external interrupts are
  injected from userspace with `KVM_INTERRUPT` (a single vector number). The guest is
  built (by `guest-sdk`) to run with the legacy-free "direct vector" contract below —
  it never touches a real lAPIC, PIC, or IOAPIC. A minimal userspace lAPIC stub in
  `dh-devices` answers the few MSR/MMIO accesses a minimal Linux guest makes during
  early boot (or we use the unikernel path which makes none). All lAPIC-stub state is
  plain Rust data ⇒ trivially snapshotted, zero hidden kernel state.
- CPUID: start from `KVM_GET_SUPPORTED_CPUID`, then apply the **determinism mask**
  (§7.2) and `KVM_SET_CPUID2`. The masked CPUID table is part of `MachineConfig` and is
  hashed into the determinism class.
- MSR filter: `KVM_X86_SET_MSR_FILTER` with default-deny for both read and write, allow
  ranges only for the MSRs the guest legitimately uses (EFER, STAR/LSTAR/CSTAR/FMASK,
  FS/GS base, SYSENTER_*, PAT, TSC_AUX). Everything else exits with
  `KVM_EXIT_X86_RDMSR`/`WRMSR` (requires `KVM_CAP_X86_USER_SPACE_MSR` +
  `KVM_MSR_EXIT_REASON_FILTER`, §2.1) and is emulated deterministically (most reads
  return a fixed config value and are logged at trace level; writes to unknown MSRs
  fault the guest with #GP — surfacing them is better than silently absorbing
  nondeterminism).
- Guest RAM: one `KVM_SET_USER_MEMORY_REGION` covering `[0, mem_size)`, plus a second
  small region for the MMIO hole (no memory backing — accesses exit with
  `KVM_EXIT_MMIO`). `KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot **only on the
  bitmap fallback path** — the dirty ring and the dirty bitmap are mutually exclusive
  per VM (enabling the ring forbids `KVM_GET_DIRTY_LOG`; setting both is an EINVAL
  trap for the implementer — see §8.2). Layout:

```
GPA range                      | What
0x0000_0000 .. mem_size        | guest RAM (memfd-backed, dirty-logged)
0xD000_0000 .. 0xD000_7000     | paravirtual MMIO window (no backing → KVM_EXIT_MMIO)
  0xD000_0000  pv-clock        (4 KiB)
  0xD000_1000  pv-pad          (4 KiB)
  0xD000_2000  pv-entropy      (4 KiB)
  0xD000_3000  (reserved)      (the guest channel is guest-sdk's detchannel: PIO
                                detcalls + a guest-RAM page, no MMIO device — §6.6)
  0xD000_4000  pv-blk          (4 KiB regs; virtio-style but simplified, see §6.5)
  0xD000_5000  pv-net (loop)   (4 KiB regs)
  0xD000_6000  debug-serial    (4 KiB; also at PIO 0x3F8 for early boot prints)
```

  PIO map (`KVM_EXIT_IO`): `0x3F8` debug serial (early boot prints) and the
  **detcall window `0xD370–0xD39F`** (guest-sdk's detchannel ABI, §6.6). All other
  ports are RAZ/WI.

  The **framebuffer window** and the **detchannel page** are ordinary guest RAM whose
  GPAs the guest publishes (the framebuffer as a `FRAMEBUFFER`-flagged region in the
  guest-sdk region manifest, §6.8; the channel page via the CHANNEL_INIT detcall,
  §6.6) — zero-copy host reads.

### 2.3 Boot protocol

v1 supports two guest types, selected by `MachineConfig.boot`:

1. **Unikernel / freestanding ELF** (the `nanokernel` tests and the
   `reference-workload` image): `dh-vmm` loads the ELF PT_LOAD segments into guest RAM,
   sets up identity-mapped 4-level page tables in low RAM, enters 64-bit mode directly
   (CR0/CR4/EFER/GDT set via `KVM_SET_SREGS`), `RIP = e_entry`, `RSI = &BootInfo`
   (a versioned struct at a fixed GPA carrying mem_size, MMIO base, cmdline bytes).
2. **Minimal Linux bzImage** via the 64-bit boot protocol: load bzImage + initramfs,
   fill `boot_params` (zero page), cmdline forced to a deterministic baseline:
   `console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock
   nohz=off highres=off init=/init` — `/init` is the initramfs shim (owned by
   guest-sdk's image layout) that execs `/sbin/detguest-agent` as PID 1. **This
   baseline is the platform's one canonical kernel cmdline, owned by this section** —
   every other repo (the WorkloadImage manifest, control-plane examples) cites it and
   never restates a variant. A manifest's `boot.cmdline` may only *append* flags from
   a whitelist (`quiet`, `loglevel=<n>`); the worker forces the baseline regardless
   and drops any conflicting or non-whitelisted flag (`BzImageBoot.cmdline`, API.md
   §2.1). `console=ttyS0` is the only console choice: it is the debug-serial
   16550-subset at PIO 0x3F8 (§6.9) — the device model has no virtio console. The
   guest kernel config is owned by `guest-sdk`/`reference-workload`; the contract is
   in INTEGRATION.md §5.

Either way, the **boot itself is deterministic** (same image + same MachineConfig ⇒
same state at any icount), so a "root snapshot" is just a snapshot taken after boot
reaches the guest-sdk ready beacon.

---

## 3. The run loop and instruction-precise event landing

This is the heart of the service. Everything observable by the guest happens at
boundaries chosen as a pure function of `(snapshot, input log)`.

### 3.1 Canonical instruction counting (`dh-detclock`)

One `perf_event_open` counter per slot, attached to the vCPU thread:

```rust
perf_event_attr {
    type_:  PERF_TYPE_HARDWARE,
    config: PERF_COUNT_HW_INSTRUCTIONS,
    pinned: 1,            // never multiplexed — startup fails if it can't be pinned
    exclude_host: 1,      // count ONLY guest-mode retired instructions
    exclude_hv: 1,
    exclude_idle: 1,
    // user+kernel of the *guest* both counted (exclude_user=0, exclude_kernel=0)
    sample_period: <armed per run segment, see below>,
    wakeup_events: 1,
    disabled: 1,
}
```

- Reads go through the mmap'd ring header (`perf_event_mmap_page`: `index`, `offset`,
  time fields ignored) with a `read()` syscall fallback; reads are only ever taken while
  the vCPU is out of guest mode, so the value is stable.
- Overflow delivery: `fcntl(F_SETOWN_EX, {F_OWNER_TID, vcpu_tid})` +
  `F_SETSIG(SIG_DETPMI)` (a chosen RT signal, e.g. `SIGRTMIN+4`). The signal handler
  does exactly one thing: sets `kvm_run.immediate_exit = 1` (the
  `KVM_CAP_IMMEDIATE_EXIT` protocol — if the signal lands outside `KVM_RUN`, the next
  `KVM_RUN` returns immediately with `EINTR`; no lost wakeups).
- `icount` (canonical) = counter value at the current pause, minus the value latched at
  the segment's base snapshot (we re-zero by `PERF_EVENT_IOC_RESET` at every
  restore/fork, so the latch is 0).

**Counting semantics.** The canonical count is whatever the hardware counter retires.
The REP and mid-emulation rules below are architectural; the interrupt rule is an
**empirically validated, per-determinism-class assumption** — not an architectural
guarantee:
- Injected external interrupts are *assumed* to retire **zero** instructions (the
  delivery itself is not a retired instruction); the guest's ISR instructions count
  normally. Exact INST_RETIRED determinism across interrupt delivery is precisely the
  property record/replay practice has found unreliable on some Intel
  parts/microcode — it must be re-validated empirically for every determinism class
  (the `counting_semantics` CI test) and is never assumed across classes. If the
  empirics fail on a given CPU/microcode, the documented fallback is to switch
  `dh-detclock` to the **retired-conditional-branches** counter with the
  `(count, RIP, RCX)` landing tuple — the boundary engine already carries RIP and RCX,
  so it survives the counter swap with minor changes. See IMPLEMENTATION-PLAN risk R2.
- A REP-prefixed string instruction retires as **one** instruction upon full
  completion. If execution stops mid-REP (PMI, single-step trap), the count has not yet
  incremented and `RIP` still points at the REP instruction. Therefore boundaries are
  the tuple `(icount, RIP)`, and the boundary engine **never declares a boundary
  mid-REP**: if refinement lands with `RIP` unchanged across a single-step (REP
  iterating), it keeps stepping until `RIP` advances. `RCX` is recorded in boundary
  diagnostics but is not part of the landing rule.
- `CPUID`, `HLT`, MMIO-exiting instructions each retire exactly once, on the resume
  that completes them. The boundary engine treats an instruction that has exited
  mid-emulation (`KVM_EXIT_MMIO` not yet completed) as **not yet retired**.

These properties are asserted empirically by the `counting_semantics` test in the
determinism suite (single-step a known instruction sequence in nanokernel, compare
counter deltas) and re-validated in CI on every host kernel/microcode bump.

### 3.2 The boundary engine: stopping at exactly icount = N

```
target N; current c = read_counter()
loop:
  d = N - c
  if d == 0 and rip_is_at_instruction_start():  -> at boundary, done
  if d > SKID_MARGIN + RESYNC_SLACK:
      arm PMI at (d - SKID_MARGIN)          # PERF_EVENT_IOC_PERIOD, then ENABLE
      KVM_RUN                               # exits via SIG_DETPMI/immediate_exit,
                                            #   or earlier for MMIO/scheduled reasons
      handle exit (MMIO etc. are serviced; they don't disturb counting)
      c = read_counter()
  else:
      KVM_SET_GUEST_DEBUG { control: ENABLE|SINGLESTEP }
      KVM_RUN                               # one KVM_EXIT_DEBUG per step
      service any interleaved MMIO exits (count unchanged until retirement)
      c = read_counter()                    # re-read; never assume +1
      (REP rule: if RIP unchanged, continue stepping without counting a boundary)
  if c > N: fatal DivergenceError::Overshoot   # P0: skid margin too small, see risks
```

- `SKID_MARGIN` default **8192** instructions, `RESYNC_SLACK` 1024. Both are
  `MachineConfig` fields and part of the determinism class only in the sense that they
  must make landing *possible*; the landed boundary itself is independent of them
  (landing at exactly N is exact regardless of how we approach it).
- Single-step cost ≈ 2–4 µs/step; 8192 steps ≈ 20–30 ms worst case. This only happens
  when an event must land (a few times per guest frame), and typical scheduled events
  are ≥ hundreds of thousands of instructions apart, so amortized overhead is small
  (see §10 targets). If profiling shows landing cost dominating, the optimization is a
  guest-mode hardware breakpoint (`KVM_GUESTDBG_USE_HW_BP`, DR0=expected RIP at N) to
  skip stepping — implement only after M5, and only with the step-verified path kept as
  the verification-mode reference.
- While single-stepping, the PMI counter stays enabled; `KVM_GUESTDBG_ENABLE` is
  dropped the moment the boundary is reached (its presence is guest-invisible: we never
  expose DR register contents to the guest, and the guest does not use debug registers —
  enforced by trapping DR writes via `KVM_GUESTDBG_USE_HW_BP` ownership; a guest DR
  write faults and is a guest-contract violation).

### 3.3 Run segments and the scheduler

`Run(until)` compiles to a sorted agenda of **stop points**:

```
agenda = merge(
    scheduled injections (icount each),         # from InjectInputs + pv timer arms
    epoch boundaries (every epoch_len icount),  # hash points (verify mode: always;
                                                #   normal mode: configurable, default on)
    goal polls (every goal_poll_period icount), # only if Run.until has a goal condition
    final stop (icount budget / vns budget converted to icount),
)
```

The loop repeatedly: land at next agenda point (§3.2) → perform the action (inject,
hash, poll, stop) → continue. **Every agenda point is a pure function of the inputs**,
so two replays compute identical agendas. `until = next_sdk_event{stream?}` adds a
dynamic stop: the detchannel doorbell detcall exit (`OUT 0xD380`, §6.6) checks a
"stop on SDK event" condition — optionally filtered to one detchannel EventKind
(API.md §2.4), with non-matching events forwarded without stopping; the doorbell exit
itself happens at a deterministic icount (guest-initiated), so the pause boundary is
deterministic too. `until = frame_budget` (API.md §2.4) likewise adds a dynamic stop:
the Nth pv-pad `FRAME_COUNTER` frame-boundary exit since run start (§6.6) — also
guest-initiated at an exact icount, recorded in the frame table, so the boundary is
deterministic. This is the platform's **only frame-quantized stop condition**, and it
guarantees the final pause lands on a FrameMark boundary (feature reads and frame
hashes are never torn); a burst scheduled as `at_frame` events plus
`frame_budget = burst frames` consumes its whole agenda before the pause, satisfying
TakeSnapshot's empty-agenda precondition (§8.1). Frame counting never feeds virtual
time (§4).

Asynchronous, **non-deterministic** stop requests (`Pause` RPC, worker shutdown) are
honored at the next exit of any kind, and then the engine **rolls forward to the next
epoch boundary** before reporting "paused". This keeps externally-caused pauses on the
deterministic grid: a `TakeSnapshot` after a `Pause` is always at an epoch boundary, so
the snapshot's icount is reproducible. (`Pause` is thus "pause soon at a deterministic
point", latency ≤ epoch_len instructions ≈ 50 ms guest time; document this in the API.)

### 3.4 Interrupt injection rule

To inject vector `v` at boundary `B`:

1. Land at `B` (§3.2).
2. Check injectability: `kvm_run.ready_for_interrupt_injection == 1` and
   `kvm_run.if_flag == 1` (and no pending exception in `KVM_GET_VCPU_EVENTS`).
3. If injectable: `KVM_INTERRUPT(v)`; the vector is delivered on the next `KVM_RUN`
   entry, before any guest instruction retires. Log the injection (it's a canonical or
   AUX record per its source).
4. If **not** injectable (IF=0, interrupt shadow, etc.): set
   `kvm_run.request_interrupt_window = 1` and continue single-stepping forward; at each
   step re-check. Deliver at the **first** injectable boundary ≥ B. Because
   injectability is a pure function of guest state, every replay defers by the identical
   number of instructions. The *actual* delivery boundary is what gets recorded in the
   AUX record (`TIMER_FIRE.delivered_icount`), and verification compares it.

This rule makes interrupt delivery exactly as deterministic as the guest itself.

---

## 4. Virtual time

- `vns = icount * clock_num / clock_den` (u128 intermediate, truncating division).
  Default `clock_num=1, clock_den=1`: one virtual nanosecond per retired instruction
  ("deterministic 1 GHz"). Per-`MachineConfig`, fixed for a VM's lifetime, recorded in
  every DHILOG header.
- **Frames are not time.** Virtual time is a pure function of icount under every stop
  condition; there is no mechanism anywhere that "advances virtual time by one frame
  period". A `frame_budget` stop (§3.3) quantizes *where a Run pauses* — the Nth
  frame-boundary exit — never how vns advances; the vns consumed per emulated frame is
  whatever the emulation cost in instructions. Callers wanting "run N frames" use
  `frame_budget = N` (API.md §2.4), never vns arithmetic.
- The guest reads time **only** through `pv-clock` (§6.2): an MMIO read exits to the
  VMM, which computes `vns` from the live counter at that exit. Since the exit happens
  at a deterministic icount, the returned value is deterministic. No log record is
  required (pure function of icount); a trace-level AUX record can be enabled for
  debugging.
- Timers: the guest arms "interrupt me at vns T" via pv-clock registers. The VMM
  converts `T → target icount = ceil(T * clock_den / clock_num)`, inserts an agenda
  point, and injects the configured vector by the §3.4 rule. Guest-armed, so
  deterministic; logged as AUX `TIMER_FIRE` for verification.
- **The hardware TSC is treated as radioactive.** Defenses, layered:
  1. The guest contract (guest-sdk) forbids RDTSC/RDTSCP as a time source; guest
     kernels use the `dh-pvclock` clocksource exclusively (`tsc=unstable` on the Linux
     cmdline demotes the TSC clocksource).
  2. CPUID mask clears `TSC_DEADLINE`, `invariant TSC` advertisement, RDTSCP
     (CPUID.80000001H:EDX[27]) — discouraging, though not preventing, raw RDTSC.
  3. CR4.TSD is set by the guest kernel so CPL>0 RDTSC faults (guest-sdk enforces).
  4. On **every VM entry** after an exit, the VMM aligns the guest TSC to `vns` at the
     entry boundary, so even a stray kernel RDTSC reads a value that is *approximately*
     virtual and drifts only between exits. **Caveat:** per-entry value writes via
     `KVM_SET_MSRS{IA32_TSC}` can engage KVM's TSC-offset synchronization/matching
     heuristics (KVM treats small-delta guest TSC writes specially) and cost
     measurably at ~3k exits/guest-second — prefer adjusting the **TSC offset**
     (`KVM_VCPU_TSC_CTRL` offset attribute) over MSR value writes; benchmark both in
     M3 before freezing the mechanism.
  5. **Verification mode is the backstop**: any stray RDTSC that observably influences
     state diverges the epoch-hash chain and is caught and bisected (the diagnostics
     specifically decode RDTSC at the divergence RIP). Long-term hardening — a tiny
     host-kernel module flipping the VMX "RDTSC exiting" execution control — is listed
     as future work in IMPLEMENTATION-PLAN.md risks; **do not** build it for v1.

---

## 5. Entropy

- One `ChaCha20Rng` per VM segment, seeded from `entropy_seed` (32 bytes) in the
  DHILOG header. The orchestrator/caller picks seeds; the hypervisor never reads host
  entropy on the replay path.
- Guest draws via `pv-entropy` (§6.3): MMIO doorbell requests `len` bytes into a
  guest-RAM buffer; the VMM fills from the PRNG at the (deterministic, guest-initiated)
  exit and appends an AUX `ENTROPY` record `{icount, len, digest8}`.
- PRNG **state** — exactly `{seed: [u8;32], stream: u64, word_pos: u128}` (56 bytes),
  `rand_chacha`'s exportable state via `get_seed`/`get_stream`/`get_word_pos` — is
  captured in every DHSNAP blob (`ENTR` section, API.md §4), so a fork resumes the
  stream exactly by default; a golden test asserts restore reproduces the next N draws
  bit-identically (IMPLEMENTATION-PLAN M4). A *new* segment may override the seed
  (new DHILOG header; `ForkRequest.entropy_seeds` / `RestoreSnapshotRequest.entropy_seed`)
  — that's how the orchestrator diversifies branches. Omitted/all-zero seeds continue
  the base snapshot or fork-point stream.
- CPUID mask clears RDRAND (leaf 1 ECX[30]) and RDSEED (leaf 7 EBX[18]). A guest that
  executes RDRAND anyway still gets hardware randomness (no trap exists without a VMX
  control KVM doesn't expose) — this is a guest-contract violation, caught by
  verification mode. The curated guest images never do it.

---

## 6. Device models (`dh-devices`)

All devices are plain Rust state machines implementing:

```rust
pub trait DetDevice {
    fn mmio_read(&mut self, off: u64, data: &mut [u8], ctx: &mut DevCtx);
    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx);
    fn snapshot(&self, w: &mut DhsnapWriter);          // versioned, per-device section
    fn restore(&mut self, r: &mut DhsnapReader) -> Result<()>;
}
```

`DevCtx` gives them: current icount, the input-log writer, guest memory access, an
interrupt-request queue (drained by the boundary engine via §3.4 — devices never inject
directly), and the entropy PRNG. **No device may read host time, host randomness, or do
host I/O on the execution path** (the blk overlay reads pre-opened, immutable files —
allowed because content is fixed by the image hash). Clippy lint + code review gate:
`std::time`, `rand::*`, and network types are deny-listed in `dh-devices`.

### 6.1 Register convention

Each device's 4 KiB window: `0x00 MAGIC` (RO, device id), `0x04 VERSION` (RO),
`0x08..` device-specific. All registers are 4- or 8-byte naturally aligned; unaligned
access ⇒ guest fault.

### 6.2 pv-clock (`0xD000_0000`)
- `0x08 VNS_LO/HI` (RO, 8B): current virtual nanoseconds.
- `0x10 ICOUNT` (RO, 8B): current icount (for guest-sdk diagnostics).
- `0x18 TIMER_DEADLINE` (RW, 8B): vns deadline; write 0 disarms. One-shot.
- `0x20 TIMER_VECTOR` (RW, 4B): vector to inject (guest picks, default 0x30).
- `0x24 FREQ_NUM / 0x28 FREQ_DEN` (RO): clock rational.

### 6.3 pv-entropy (`0xD000_2000`)
- `0x08 BUF_GPA` (RW, 8B), `0x10 LEN` (RW, 4B), `0x14 DOORBELL` (WO): on write, VMM
  fills `LEN` bytes at `BUF_GPA` from the PRNG, sets `0x18 STATUS=1`. Synchronous —
  data is in place when the MMIO write instruction retires.

### 6.4 pv-pad (`0xD000_1000`) — controller input
- Model: a **latch** per pad port (4 ports). `latch[p]` is a `u32` button bitmask
  (semantics owned by `reference-workload`'s pad mapping; bit assignment travels in the
  feature map, not here). The latch changes **only** when a canonical `PAD_SET` log
  record lands at its icount; injection while paused at the boundary = writing the
  latch + an optional edge interrupt.
- `0x08 PAD0..0x14 PAD3` (RO, 4B each): current latch values. **This latch is the
  platform's only pad-input path**: the SDK reads it once per emulated video frame
  (guest-sdk's `poll_input` wraps the latch read); the detchannel's ring I carries no
  pad data (it exists for generic userspace inputs in the bug-hunting
  generalization). The read is an MMIO exit at a deterministic icount, returning a
  value that changed only at logged icounts ⇒ fully deterministic.
- `0x18 IRQ_VECTOR` (RW): optional "pad changed" edge interrupt (default disabled; the
  demo harness polls per frame).
- `0x1C FRAME_COUNTER` (RW, 4B): the guest increments this each emulated video frame
  as one half of the FRAME_MARK consistency rule (§6.6); the host samples it for
  `GetFramebuffer` metadata and for converting "per-frame" input schedules to icounts
  (see API.md `ScheduledEvent.at_frame`): a `PAD_SET` scheduled `at_frame=F` is landed
  at the icount of the FRAME_COUNTER MMIO exit that marks frame F (the frame-boundary
  exit, §6.6), which is deterministic. **`at_frame` values are absolute FRAME_COUNTER
  values** (normative, API.md §2.3): the counter is device state snapshotted in DHSNAP
  `PADD`, so it persists across snapshot/restore and is strictly increasing along a
  lineage — never segment-relative. Each segment's frame table (DHILOG AUX
  `FRAME_MARK`) maps absolute F → segment-relative icount; the current value is
  returned in `RestoreSnapshotResponse`/`TakeSnapshotResponse` so callers schedule
  `at_frame = frame_counter + offset`. `Run{frame_budget: N}` pauses at the Nth
  frame-boundary exit since run start (§3.3).

### 6.5 pv-blk (`0xD000_4000`) — read-only base + CoW overlay
- Simplified virtio-blk-like: a one-deep request register set (no rings — single vCPU,
  synchronous completion keeps it deterministic and simple):
  `0x08 SECTOR` (8B), `0x10 BUF_GPA` (8B), `0x18 COUNT` (4B, sectors),
  `0x1C CMD` (WO: 1=read, 2=write, 3=flush), `0x20 STATUS` (RO).
- Backend: base image file (opened `O_RDONLY`, content hash recorded in MachineConfig)
  + **overlay**: 64 KiB clusters, an in-memory `HashMap<cluster_idx, Box<[u8;65536]>>`
  populated on first write (read-modify-write from base). Reads check overlay first.
  Completion is synchronous within the MMIO-write emulation ⇒ zero timing variance.
- Snapshot: the overlay's dirty clusters are serialized into the DHSNAP device section
  as `(cluster_idx, blake3, bytes)`; incremental snapshots include only clusters
  dirtied since the parent (the codec stores a parent-relative cluster set, mirroring
  page handling). Typical demo guests write almost nothing to disk.

### 6.6 The guest channel: guest-sdk's detchannel (host side)

The guest↔host channel is **owned by guest-sdk**: one 2 MiB guest-RAM page holding a
header, the region manifest, and four SPSC rings (C and I host→guest, A and W
guest→host), driven by PIO "detcall" registers at `0xD370–0xD39F`. The page layout,
ring framing, event kinds, the manifest format, and the detcall register ABI are
specified in `../guest-sdk/ARCHITECTURE.md` §2 and `API.md` §3–§5 and are **never
restated here normatively**. This service links `detguest-host` and implements the
host-side obligations:

- **PIO detcall handler** (`KVM_EXIT_IO`, ports `0xD370–0xD39F`): IDENT (`0xD370`),
  CHANNEL_INIT (`0xD374/0xD378/0xD37C`), DOORBELL (`0xD380`), INJECT (`0xD384`),
  QUIESCE_ACK (`0xD388`) — register semantics per guest-sdk API.md §5. Every detcall
  is a synchronous, guest-initiated exit at an exact instruction boundary; every `IN`
  return value is a canonical DHILOG `DEV_EVENT/PIO_ANSWER` record (API.md §3.3).
  (The host-facing initiator of the quiesce protocol is the `Quiesce` RPC, API.md
  §2.10 — Phase 8, optional; nothing in the v1 loop issues one.)
- **Channel discovery:** at CHANNEL_INIT the handler validates and attaches the page
  (`detguest_host::Channel::attach`), records the channel base GPA in per-slot
  metadata (DHSNAP `EVTC` section), and reads the **region manifest**
  (seqlock-consistent) — this is where the capture engine (§6.10) gets its
  region→extent resolution. The manifest is guest RAM: after any restore the host
  re-attaches at the recorded GPA and re-reads it, with no event replay needed.
- **Doorbell drain:** `OUT 0xD380` drains the indicated guest→host rings inside the
  exit (host reads guest RAM directly — zero copy until the gRPC boundary), stamps
  each record with the doorbell icount, appends AUX `SDK_EVENT` records (digest only)
  to the log, and forwards payloads to `StreamGuestEvents` subscribers. Rings are
  also drained at every pause boundary.
- **Host mutations are inputs:** every host-side mutation of channel memory — pushing
  a command into ring C or an input into ring I, bumping a consumer index after a
  drain, the value answered to an `IN` detcall — is recorded as a canonical DHILOG
  `DEV_EVENT` record through the `ChannelWriteSink` hook (encodings: API.md §3.3).
  The host touches channel memory **only while the vCPU is paused** (a pause boundary
  or inside a detcall exit); replay re-applies each mutation at its recorded icount.
  This is guest-sdk's load-bearing invariant (its ARCHITECTURE §2/§7), honored here.
- **FRAME_MARK consistency rule (normative; ordering owned by guest-sdk API.md §1.6):**
  the SDK signals a frame boundary in exactly this order — (1) the framebuffer for
  frame `F` is fully written, (2) the `FrameMark` event (a detchannel EventKind,
  guest-sdk API.md §3.1) carrying `F` is written to ring W with a release-stored
  producer index, (3) the pv-pad `FRAME_COUNTER` register (§6.4) is MMIO-written to
  `F` — and that MMIO write **is** the frame-boundary VM exit. The host records
  `F → exit icount` in the per-segment frame table (mirrored as the DHILOG AUX
  `FRAME_MARK` record) and may drain ring W inside the same exit — the record is
  guaranteed visible because it precedes the write. No doorbell is rung for frame
  marks: one exit per frame. `at_frame` scheduling, `frame_budget` stops,
  `next_sdk_event{stream}` stops, `GetFramebuffer`, and `RunWithFrameCapture` all key
  off this table. At the exit,
  the ring-W `FrameMark` frame index MUST equal the written `FRAME_COUNTER` value —
  a mismatch is a guest-contract violation (`FAULTED`).
- **No asynchronous host access, ever:** draining and pushing happen only inside
  guest-initiated exits or while paused ⇒ determinism preserved.

### 6.7 pv-net loopback (`0xD000_5000`)
- Same one-deep register style as pv-blk: guest TX (`BUF_GPA/LEN/DOORBELL`) captures the
  frame into the event stream (AUX `NET_TX` digest record + full frame to subscribers);
  guest RX happens only when a canonical `NET_RX` log record lands at its icount: the
  frame bytes are copied into a guest-published RX buffer and the RX vector is injected
  per §3.4. No host networking anywhere. (Demo workload doesn't use this; it exists for
  the bug-hunting generalization.)

### 6.8 Framebuffer window
- Not a device: the guest (emulator harness via guest-sdk) renders into a contiguous
  guest-RAM region published as a `FRAMEBUFFER`-flagged entry in the guest-sdk region
  manifest (`RegionFlags::FRAMEBUFFER`, guest-sdk API.md §1.5), with
  `{width, height, stride, pixel_format}` in a small descriptor struct at the region
  start (layout pinned by the region's `layout_version`). `GetFramebuffer` and the
  capture engine (§6.10) resolve the region through the manifest and read descriptor
  + pixels straight out of the slot's memory mapping while paused. Zero copy on-host;
  one copy (or one lz4 pass) onto the wire.

### 6.9 debug-serial
- 16550-subset at PIO `0x3F8` + MMIO mirror. Output-only (input would be
  nondeterministic); bytes go to the slot's JSON log. Reads of RX registers return 0.
  Disabled (writes swallowed) in verification mode *and* normal mode alike for state
  purposes — serial output never enters the state hash.

### 6.10 Capture engine (C-requirements, normative)

The per-step feature/framebuffer capture path is **owned by this service** (MAP.md
dataflow step 4). It is invoked via the optional `CaptureSpec` on `Run` and
`TakeSnapshot` (API.md §2.4–2.5) and returns `feature_bytes` + `fb_lz4` inline in
those responses. The orchestrator forwards both inline to state-scorer in
`ScoreBatch` — **the scorer never touches workers** (no pull path exists).

| Req | Behavior |
|---|---|
| C1 | **Feature-map-agnostic.** This service never parses feature maps (reference-workload's schema). The orchestrator compiles the experiment's feature map into the flat `ExtractRange{region, layout_version, offset, len}` list once at experiment start; the hypervisor only resolves names and reads bytes. |
| C2 | **Region resolution.** Each `ExtractRange` resolves through the guest-sdk region manifest, read at CHANNEL_INIT and re-read after every restore (§6.6). An unknown region name or a `layout_version` mismatch fails the call with `FAILED_PRECONDITION` — never silently read garbage (guest-sdk's layout-versioning rule). |
| C3 | **Packing.** `feature_bytes` is the requested ranges' bytes concatenated **in request order**, no padding, no framing — the caller compiled the list, so it knows the layout. |
| C4 | **Framebuffer.** `CaptureSpec.framebuffer = true` reads the `FRAMEBUFFER`-flagged region (§6.8) at the pause boundary and returns lz4-compressed pixels plus `FbInfo` (dimensions, format, pv-pad `FRAME_COUNTER`). |
| C5 | **Capture-neutrality.** Capture runs while paused at the boundary and is read-only: it MUST NOT perturb execution, the DHILOG, or the state hash. A capture run and a no-capture run of the same `(snapshot, inputs)` produce identical child refs and epoch hashes (CI-tested, M6). |
| C6 | **Cost.** Inside the §10 per-job budget: feature reads (≤ 64 KiB typical) < 1 ms; framebuffer read + lz4 < 3 ms. |

---

## 7. Sources-of-nondeterminism ledger

### 7.1 Closed by construction
host wall clock (virtual time), interrupt timing (boundary engine), entropy (seeded
PRNG), device I/O content/ordering (deterministic models, synchronous completion), SMP
races (single vCPU), guest-visible PMU/MSR surprises (default-deny MSR filter), dirty
host state leaking in (every input is in MachineConfig, the snapshot, or the log).

### 7.2 Closed by CPUID/config masking
Cleared bits (non-exhaustive, the code is the source of truth; every cleared bit gets a
comment naming the nondeterminism it closes): RDRAND, RDSEED, TSC_DEADLINE, ARAT,
MWAIT/MONITOR, x2APIC (no APIC at all in the direct-vector contract), PDCM/PMU
(guest must not see counters — also force `KVM_CAP_PMU_CAPABILITY` /
`KVM_PMU_CAP_DISABLE` so the in-guest vPMU is off and cannot interact with our host
counter), TM/turbo/thermal leaves zeroed, KVM paravirt leaves (0x4000_00xx) **removed
entirely** (no kvmclock, no PV spinlocks, no async PF, no steal time), AVX512 family
optionally masked to the fleet's lowest common denominator (determinism-class concern,
not a correctness one).

### 7.3 Residual risks (accepted, monitored by verification mode)
Stray RDTSC between exits (§4), stray RDRAND despite CPUID (§5), microcode/kernel
updates changing counting semantics (§3.1 empirics in CI), PMU overcount errata, host
memory corruption. Verification mode exists because this list can never be proven
empty: trust, but re-execute.

### 7.4 Host configuration (deployment requirement, checked by `dh-workerd --preflight`)
- Kernel cmdline: `isolcpus=managed_irq,domain,<slot cores> nohz_full=<slot cores>
  rcu_nocbs=<slot cores>`; SMT siblings of slot cores left idle (or SMT off).
- `kvm_intel` module params: `enable_pml=1` (dirty logging accel ok), `ple_gap=0`
  (PLE exits off — single vCPU anyway).
- THP **off** for slot memfds (`MADV_NOHUGEPAGE`) — 4 KiB-exact dirty granularity.
- NMI watchdog off (`kernel.nmi_watchdog=0`) — it eats a PMU counter and injects host
  NMIs; perf paranoid level permitting `perf_event_open` by the service user.
- Pin host kernel + microcode versions; both are part of the determinism-class tuple
  returned in `TakeSnapshotResponse` and persisted into lineage-node attrs by the
  orchestrator (API.md §5.1).

---

## 8. Snapshot, restore, fork (`dh-snapshot`)

### 8.1 What a snapshot contains

```
Snapshot = Manifest (the store's .spm container, built by snapstore-client;
                     it carries NO metadata section — API.md §5.1):
  ├─ parent snapshot ref | none      (DELTA flag; FULL for roots)
  ├─ page entry table: [(page_idx, page_hash)] — pages dirtied since parent
  │                    (or all, for roots)
  └─ device blob (DHSNAP, opaque to the store), containing:
       machine config: canonical MachineConfig encoding (MCFG section — CPUID table,
               mem size, clock rational, device set; the machine_config_hash preimage)
       vCPU:   KVM_GET_REGS, KVM_GET_SREGS2, KVM_GET_FPU, KVM_GET_XSAVE2,
               KVM_GET_XCRS, KVM_GET_MSRS (explicit list, see below),
               KVM_GET_VCPU_EVENTS, KVM_GET_DEBUGREGS
       lapic-stub + every DetDevice section (versioned each)
       virtual-time: icount_at_capture (=0 going forward: icount is segment-relative;
               we store cumulative_icount u64 for diagnostics), vns, pending agenda
               (MUST be empty — snapshots only at quiescent boundaries with no
               unconsumed scheduled events; TakeSnapshot fails otherwise)
       entropy: ChaCha20 state (seed / stream / word_pos — ENTR, API.md §4)
       hashchain: current chain value + epoch index
```

MSR capture list (explicit, versioned in code): EFER, STAR, LSTAR, CSTAR, SFMASK,
KERNEL_GS_BASE, FS_BASE, GS_BASE, SYSENTER_{CS,ESP,EIP}, PAT, TSC_AUX, IA32_TSC
(normalized: we *write* vns on restore rather than trusting the captured value),
SPEC_CTRL. Capturing an MSR not on the list that the guest wrote would have faulted at
write time (§2.2 filter), so the list is complete by construction.

XSAVE normalization: `KVM_GET_XSAVE2` output can vary byte-wise for logically-equal
state (init-optimization). The DHSNAP codec **canonicalizes**: for each XSAVE component
whose XSTATE_BV bit is clear, the component area is zeroed in the blob. Restore feeds
the canonical form to `KVM_SET_XSAVE`; equality of blobs then implies equality of
logical state and vice versa. The state hash uses the canonical form.

### 8.2 Dirty-page tracking
- Primary: **dirty ring** (`KVM_CAP_DIRTY_LOG_RING_ACQ_REL`, ring size 65536 entries;
  EMPIRICS, iteration 84: the kernel reserves 64 + 512 (PML) entries on x86 and
  rejects rings below that floor — 1024 is the smallest legal ring on the lab box,
  which is what the 28i chaos acceptance forces, not the originally-sketched 512)
  drained at every pause; entries are reset with `KVM_RESET_DIRTY_RINGS` after harvest.
  Ring-full causes a guest exit (`KVM_EXIT_DIRTY_RING_FULL`) which we service and
  resume — this exit is *host-visible only* and does not perturb guest state (verified
  by a dedicated determinism test that forces tiny rings).
- Fallback: bitmap `KVM_GET_DIRTY_LOG` + `KVM_CLEAR_DIRTY_LOG`
  (manual-protect mode) over the RAM region — only on this path is
  `KVM_MEM_LOG_DIRTY_PAGES` set on the memslot (the ring and the bitmap are mutually
  exclusive per VM, §2.2).
- The dirty set is maintained as a per-slot `RoaringBitmap<page_idx>` accumulated since
  the last `TakeSnapshot`. `TakeSnapshot` = land on boundary → drain ring → hash every
  dirty page (blake3, rayon-parallel — the hashes go into the manifest entry table) →
  ship the **bare page bytes** back-to-back in a memfd over the snapshot-store page
  channel (`PUT_BATCH`; the server hashes and dedups itself, no indices on the wire —
  API.md §5.1) → cross-check `batch_blake3` → `PutSnapshot(.spm container)` →
  receive snapshot ref → clear dirty set. Pages are *read* directly from the live
  mapping while paused — no shadow copy needed.

### 8.3 Restore
`RestoreSnapshot(ref)`:
1. `GetSnapshot(ref)` from snapshot-store → manifest chain flattened server-side into a
   full page list.
2. Populate guest RAM: pages stream over the fast-path channel into the slot's memfd
   mapping (`pwritev` into the mapping; large restores use the store's
   materialized-file path below).
3. Decode DHSNAP → `KVM_SET_*` everything (order matters: SREGS2 before REGS before
   VCPU_EVENTS; XSAVE before XCRS is **wrong**, set XCRS then XSAVE; set MSRs last,
   IA32_TSC ← vns), restore device models, PRNG, hashchain; `PERF_EVENT_IOC_RESET` the
   counter; dirty set cleared; state = Paused at a boundary with icount 0 (segment-
   relative).

### 8.4 Fork (the hot path)
The MAP.md milestone-1 operation. Two tiers:

- **Tier A — same-worker fork (CoW, the common case).** The parent slot's RAM is a
  memfd. Fork: create child slot whose RAM mapping is `mmap(MAP_PRIVATE)` of the
  parent's memfd, sealed with **`F_SEAL_FUTURE_WRITE`** once the parent pauses.
  (`F_SEAL_WRITE` is unusable here: it fails `EBUSY` while *any* writable shared
  mapping of the file exists, and the parent's KVM-registered guest-RAM mapping is
  exactly such a mapping — pausing the vCPU does not unmap it. `F_SEAL_FUTURE_WRITE`
  blocks **new** writable mappings, protecting children's views; the guard against
  the parent itself writing is the **`Frozen` slot-state machine**: a paused parent
  with live children is `Frozen{children:n}` and cannot run. That guard is
  software-enforced, not kernel-enforced — see risk R9.) Child writes CoW in
  anonymous memory at 4 KiB granularity via the private mapping. vCPU + device state:
  decode the parent's in-memory DHSNAP (cheap, ~tens of KiB). Cost target: **< 10 ms**
  (no page copies, one mmap + KVM state stuffing).
  KVM detail: each child is its own `VmFd` (KVM VMs don't share EPT across fds; CoW
  happens at the host-pagetable level — EPT violations fault pages in lazily; first-
  touch cost is the page-fault, measured in §10).
  Entropy detail: the in-memory DHSNAP restores the fork-point ENTR state first.
  Public `ForkRequest.entropy_seeds` may then reseed each child segment; empty or
  all-zero seeds keep the fork snapshot-equivalent, non-zero seeds start fresh
  deterministic child streams.
- **Tier B — cross-worker / cold restore.** §8.3 via snapshot-store. The store's
  materialized-file fast path (`ResolvePages` to a per-ref read-only flat file on NVMe,
  cached) lets restore be a single `mmap(MAP_PRIVATE)` of that file: lazy page-in from
  page cache, CoW on write. Cost target: **< 150 ms** warm cache for the **128 MiB**
  demo guest (MAP.md canonical figure).

After either fork tier the child re-zeroes icount, opens a fresh DHILOG (header's
`base_snapshot_id` = parent ref), and is ready for `InjectInputs` + `Run`.

### 8.5 State hash (normative definition)

```
H_0   = blake3("dh-statehash-v1" || machine_config_hash || base_snapshot_ref)
H_i+1 = blake3(H_i
        || canonical vCPU blob (DHSNAP vCPU section bytes, canonicalized §8.1)
        || device sections bytes
        || for each page dirtied since previous hash point, ascending idx:
              le64(page_idx) || page bytes (4096)
        || le64(icount) || le64(vns))
```

Computed at every epoch boundary and at every final pause. "Dirtied since previous hash
point" uses the same dirty-ring harvest as snapshots (drained, not cleared-to-store).
The chain value (not just the last link) is the **state hash** exchanged with other
services; comparing chains compares full execution histories, not just endpoints.

---

## 9. Worker model (`dh-worker`)

- One `dh-workerd` process per host. At startup: preflight checks (§7.4), open KVM,
  build the slot table (`--slots N`, default `physical_cores - 2`; each slot gets a
  dedicated isolated core for its vCPU thread + the IO/gRPC threads share the
  housekeeping cores), connect to snapshot-store UDS, serve gRPC.
- A slot's lifecycle: `Empty → Created (CreateVm/RestoreSnapshot/Fork) → Paused ⇄
  Running → Frozen (parent of live CoW children) → Empty (DestroyVm)`. All RPCs carry
  `slot_id`; a `lease_token` (issued at create/restore/fork, echoed by mutating RPCs)
  prevents two orchestrator retries from interleaving on one slot.
- Threads: 1 vCPU thread per active slot (pinned, `SCHED_FIFO` prio 10 on its isolated
  core); device emulation runs **on the vCPU thread** inside exits (determinism: no
  cross-thread device state); per-slot snapshot hashing fans out to a rayon pool on
  housekeeping cores *while paused*; tonic runs on housekeeping cores.
- Metrics (Prometheus): per-slot icount rate, exits/sec by reason, landing
  single-steps/sec, snapshot ms, fork ms, restore ms, dirty pages per snapshot,
  verification failures (alert at > 0), PMI skid histogram.
- **Image-cache population.** `CreateVm` requires `base_image_hash` to already be in
  the local image cache (`/var/lib/dh/images/`, keyed by BLAKE3). In Phases 3–5 the
  cache is populated **out-of-band** (the operator copies the image; the worker only
  verifies the hash on open). From Phase 6, `dh-workerd` fetches missing images over
  HTTP from control-plane's blob store using the `svc-hypervisor-host` service token,
  verifies the BLAKE3 against `base_image_hash`, and caches locally. The fetch path is
  deployment plumbing, never on the execution path — a missing, unfetched image fails
  `CreateVm` with `NOT_FOUND`.

---

## 10. Performance targets & engineering

Budget for the MAP.md principle-2 number — **fork + run 1 guest-second + snapshot ≪ 1 s
wall** — on the Intel box, **128 MiB demo guest** (MAP.md canonical figure), demo
workload:

| Stage | Target (p50) | How |
|---|---|---|
| Fork (tier A) | < 10 ms | CoW mmap, no copies; DHSNAP decode ~50 µs; KVM state stuffing ~1 ms |
| Run 1 guest-second (1e9 instructions at 1:1 clock) | < 400 ms | Native execution ≥ 3 GIPS retired on modern Intel cores; exit budget: ≤ 3k exits/guest-s (pad reads and detchannel frame exits at ~1/emulated frame, assuming the demo's ~60 emulated frames per guest-second — a workload cost estimate, not a pacing mechanism: frames are not time, §4; clock reads ≤ 1k/s) at ~2 µs each ≈ 6 ms; landing ≤ 200 injections/guest-s × ≤ skid-margin steps only when adjacent (typ. ≤ 50 steps) ≈ 20 ms; epoch hashing 20 epochs × (regs + ~2k dirty pages) ≈ 30 ms with rayon |
| TakeSnapshot (incremental, ≤ 8k dirty pages) | < 15 ms | parallel blake3 ≈ 1 GB/s/core; page channel is memfd hand-off, no copies; store dedups |
| Capture engine: feature ranges (≤ 64 KiB) | < 1 ms | direct mapping read via manifest extents (§6.10) |
| Capture engine: framebuffer (sized for 512×448×4 ≈ 900 KiB — deliberate worst-case headroom; the canonical demo framebuffer is 256×224 XRGB8888, stride 1024 = 229,376 B) | < 3 ms | one lz4 pass, one copy to wire |
| **Total per exploration job** | **< 450 ms** | leaves headroom for the < 1 s budget incl. orchestrator/scorer latency |

These derive from MAP.md's canonical figures: **128 MiB demo guest**, hypervisor
per-job budget **< 450 ms**, with snapshot-store's **≤ 100 ms** storage share inside
it (the < 15 ms TakeSnapshot row is this service's hand-off cost; the store's
persistence work happens within its own ≤ 100 ms share).

Throughput knob: epochs (hashing) can be lowered to "final-only" in exploration mode —
exploration jobs only need the final state hash; full chains matter in verification.
Default: epochs on (cheap insurance); config flag `hash_epochs=final_only` available.

---

## 11. Future work: deterministic multi-vCPU

Not in v1. The documented path (for the eventual design doc, not implementation now):

- **Why it's hard:** with >1 vCPU, the interleaving of memory accesses is decided by
  hardware/scheduler, not by us; an icount-pair `(icount_0, icount_1)` does not identify
  a unique global state because the cores interleave between boundaries.
- **Path A — deterministic round-robin (semantics-preserving, slow):** run vCPUs one at
  a time in fixed quanta of Q retired instructions each (boundary engine per vCPU,
  identical machinery). Fully deterministic; throughput ≈ 1/n of native; shared-memory
  interleavings limited to quantum granularity (guest code requiring finer interleaving
  for progress, e.g. spinlocks, needs quantum-expiry fairness plus HLT/PAUSE exit
  hooks). This is the v2 default candidate because it reuses §3 wholesale.
- **Path B — chunk-based speculation with conflict detection:** run vCPUs concurrently
  in chunks, track read/write page sets per chunk (EPT permissions), commit chunks in a
  deterministic order, roll back conflicting chunks. Big engineering lift (per-vCPU EPT
  views, rollback memory), large win; literature exists in deterministic-multithreading
  research. Only worth it if the workload portfolio demands SMP guests.
- **Logged-interleaving record/replay** (record nondeterministic interleavings, replay
  them) is rejected: it makes *recordings* replayable but forks non-deterministic at
  generation time, which breaks the exploration model (forks must be deterministic
  going *forward*, not just replayable backward).

Single-vCPU consequences accepted in v1: guest images are built UP (uniprocessor);
the emulator demo is single-threaded by nature.
