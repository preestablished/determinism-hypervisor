#!/bin/bash
# Project: determinism-hypervisor — Phase 1 (Deterministic Execution, single timeline)
# Generated: 2026-06-09
#
# Creates the Beads task graph for hypervisor M0-completion + M1 + M2 + M3,
# the nanokernel guest-program track, and the determinism CI track.
# Grounding docs (cite-checked): .agents/docs/MAP.md (clean-room boundary),
# .agents/docs/determinism-hypervisor/{ARCHITECTURE,IMPLEMENTATION-PLAN,API}.md,
# .agents/docs/phases/phase-1-deterministic-execution.md, .agents/docs/guest-sdk/.
#
# Clean-room boundary (MAP.md, normative): implementation agents use ONLY this
# project's docs, public HW/OS/KVM/Rust/crate documentation, and operator-supplied
# artifacts. If a requirement can't be met from that set, stop and file a
# documentation issue.
#
# Every bead description states whether its acceptance is HOST-RUNNABLE (any
# machine incl. macOS/aarch64 dev hosts) or HARDWARE-GATED (kvm-intel runner /
# Intel box only), plus the file surface to reserve via MCP Agent Mail.
#
# Sequencing guard (phase doc): M4 (snapshots) does not start until the Phase 1
# determinism gate is green — encoded as the PHASE1_EXIT bead at the bottom.

set -e
cd "$(dirname "$0")"

# Initialize beads if needed
if [ ! -d ".beads" ]; then
    bd init
fi

# Idempotency guard: never double-create the graph on a re-run
if bd list --limit 0 2>/dev/null | grep -q "Phase 1 exit gate"; then
    echo "Phase 1 bead graph already exists — nothing to do."
    exit 0
fi

echo "Creating Phase 1 beads..."

# ============================================================
# M0 completion — workspace reconciliation, boot stub, preflight
# (Phase 0 nominally passed, but M0 acceptance artifacts do not
#  exist: crates are empty stubs; IMPLEMENTATION-PLAN M0.)
# ============================================================

CRATE_LAYOUT=$(bd create "Reconcile workspace to ARCHITECTURE §1 crate layout" \
  -d "ARCH §1 is normative: crates/{dh-vmm,dh-detclock,dh-devices,dh-inputlog,dh-snapshot,dh-verify,dh-proto,dh-worker}, tools/dh-cli, tests/nanokernel, tests/determinism, proto/hypervisor.proto. Scaffolded dh-types/dh-kvm/dh-smoke appear nowhere in ARCH §1 — decide disposition (fold dh-kvm into dh-vmm, dh-types into dh-vmm or keep as internal util, retire dh-smoke into tests/) and record the decision in README. Enforce ARCH §1 dependency rules: dh-vmm depends on dh-detclock/dh-devices/dh-inputlog; dh-worker depends on everything; nothing depends on dh-worker; dh-inputlog deps = blake3 only. Keep determinism-proto path dep on ../control-plane working. HOST-RUNNABLE (cargo build/test, any arch). Files: Cargo.toml, crates/**, tools/**, tests/**" \
  -p 0 -l m0-bootstrap -t task --silent)

HOSTCFG=$(bd create "Apply and document ARCH §7.4 host configuration on the Intel box" \
  -d "Kernel cmdline isolcpus=managed_irq,domain,<slot cores> nohz_full rcu_nocbs; SMT siblings of slot cores idle (or SMT off); kvm_intel enable_pml=1 ple_gap=0; THP off for slot memfds (MADV_NOHUGEPAGE convention); kernel.nmi_watchdog=0 (it eats a PMU counter and injects host NMIs — gates M2 skid acceptance); perf_event_paranoid permitting perf_event_open for the service user. Record kernel + microcode versions as the baseline for ci/determinism-class.lock. Ops task, HARDWARE-GATED (Intel box). Files: docs/ops/** (runbook notes)" \
  -p 0 -l m0-bootstrap -t chore --silent)

KVM_FOUNDATION=$(bd create "dh-vmm KVM bring-up: caps check, VM/vCPU, guest memory, exit dispatch" \
  -d "Open /dev/kvm; assert the full ARCH §2.1 required-capability table (hard-fail if absent) and the §2.1 forbidden list (no KVM_CREATE_IRQCHIP, no KVM_CREATE_PIT2, no kvmclock, no KVM_CAP_X86_DISABLE_EXITS); create VM + exactly one vCPU; guest RAM as one memfd-backed GuestMemoryMmap region [0,mem_size) with MADV_NOHUGEPAGE + the no-backing MMIO hole at 0xD000_0000..0xD000_7000 (KVM_EXIT_MMIO) per the §2.2 GPA map; PIO map: 0x3F8 serial + detcall window 0xD370-0xD39F, all other ports RAZ/WI; KVM_RUN exit-dispatch skeleton. Crates: kvm-ioctls, kvm-bindings, vm-memory, vmm-sys-util per ARCH §1. HARDWARE-GATED (needs /dev/kvm; unit-test struct layout host-runnable). Files: crates/dh-vmm/**" \
  -p 0 -l m0-bootstrap -t task --silent)
bd dep add $KVM_FOUNDATION $CRATE_LAYOUT

NK_TOOLCHAIN=$(bd create "Nanokernel build pipeline: build.rs + nasm/ld freestanding x86_64 guests" \
  -d "tests/nanokernel per ARCH §1: tiny freestanding x86_64 test guests (no libc, ~2 KiB), built by build.rs with nasm/ld; BootInfo consumption per ARCH §2.3 (RSI = &BootInfo versioned struct: mem_size, MMIO base, cmdline). Shared linker script + entry crt. Build must work on any host (nasm cross-assembles x86; aarch64 dev hosts incl.) — HOST-RUNNABLE build, execution of outputs HARDWARE-GATED. Files: tests/nanokernel/**" \
  -p 0 -l nanokernel -t task --silent)
bd dep add $NK_TOOLCHAIN $CRATE_LAYOUT

HELLO_STUB=$(bd create "hello.elf: real-mode→long-mode boot stub, serial print + HLT" \
  -d "IMPLEMENTATION-PLAN M0: ~20-line stub that writes to debug-serial (PIO 0x3F8) and HLTs, built as tests/nanokernel/hello.elf by the nanokernel pipeline. Acceptance is via the dh-cli boot bead. HOST-RUNNABLE build; run HARDWARE-GATED. Files: tests/nanokernel/hello*" \
  -p 0 -l nanokernel -t task --silent)
bd dep add $HELLO_STUB $NK_TOOLCHAIN

CLI_BOOT=$(bd create "tools/dh-cli boot subcommand + minimal early serial sink" \
  -d "IMPLEMENTATION-PLAN M0 acceptance: 'dh-cli boot tests/nanokernel/hello.elf' prints the expected bytes and exits. Minimal loader for the stub, run-until-HLT loop, PIO 0x3F8 byte sink to stdout/JSON log. dh-cli is the Phase-1 driver (full gRPC worker surface is M6). HARDWARE-GATED (kvm-intel). Files: tools/dh-cli/**" \
  -p 0 -l m0-bootstrap -t task --silent)
bd dep add $CLI_BOOT $KVM_FOUNDATION
bd dep add $CLI_BOOT $HELLO_STUB

PREFLIGHT=$(bd create "Implement dh-workerd --preflight checker" \
  -d "IMPLEMENTATION-PLAN M0: checks ARCH §7.4 host config (isolcpus/nohz_full/rcu_nocbs, nmi_watchdog=0, THP, kvm_intel params, perf paranoid) AND the full §2.1 capability table including KVM_CAP_X86_USER_SPACE_MSR enabled with KVM_MSR_EXIT_REASON_FILTER; fails loudly on a stock kernel config or any missing capability. Lives in dh-worker. HARDWARE-GATED acceptance (must pass on the box, fail on stock); check logic unit-testable host-side. Files: crates/dh-worker/**" \
  -p 0 -l m0-bootstrap -t task --silent)
bd dep add $PREFLIGHT $KVM_FOUNDATION
bd dep add $PREFLIGHT $HOSTCFG

# ============================================================
# Determinism CI track
# ============================================================

CI_RUNNER=$(bd create "Register self-hosted GitHub runner labeled kvm-intel on the Intel box" \
  -d "Runner user needs /dev/kvm and perf_event access; document service install, restart, and isolation from slot cores (runs on housekeeping cores). Required by every determinism/landing job (IMPLEMENTATION-PLAN testing strategy). Ops task, HARDWARE-GATED. Files: docs/ops/**" \
  -p 0 -l determinism-ci -t chore --silent)
bd dep add $CI_RUNNER $HOSTCFG

CI_SPLIT=$(bd create "Split CI workflow: host-runnable jobs vs kvm-intel-gated jobs" \
  -d "Current .github/workflows/ci.yaml is ubuntu-latest-only. Split: (a) host job — fmt, clippy, build, unit-layer tests (codecs, agenda math, vns rational math) on ubuntu-latest AND aarch64 (must build/run there per IMPLEMENTATION-PLAN testing strategy — Spark-side devs touch shared code); (b) kvm-intel-gated job — KVM integration + determinism tests, runs-on [self-hosted, kvm-intel]. Owns ALL .github/workflows edits in this wave: keep ../control-plane and add ../guest-sdk sibling checkout (for the detguest-host path dep). HOST-RUNNABLE to author; gated leg verified on runner. Files: .github/workflows/**" \
  -p 0 -l determinism-ci -t task --silent)
bd dep add $CI_SPLIT $CRATE_LAYOUT
bd dep add $CI_SPLIT $CI_RUNNER

CI_LOCK=$(bd create "ci/determinism-class.lock + nightly host-drift check + re-baseline procedure" \
  -d "Record runner kernel + microcode versions in repo (ci/determinism-class.lock); nightly job fails if the host drifted from the lock without a deliberate bump; write the re-baseline procedure doc (a bump re-baselines the record/replay corpus — it is the procedure for absorbing host changes, not an incident; IMPLEMENTATION-PLAN testing strategy + risk R2). HARDWARE-GATED job; lock format host-runnable. Files: ci/**, .github/workflows/**, docs/**" \
  -p 1 -l determinism-ci -t task --silent)
bd dep add $CI_LOCK $CI_RUNNER
bd dep add $CI_LOCK $CI_SPLIT

# ============================================================
# M1 — nanokernel guest + pv devices + boot from image
# ============================================================

MACHINECONFIG=$(bd create "MachineConfig type + canonical encoding (machine_config_hash preimage)" \
  -d "Boot selection (ELF for Phase 1; BzImageBoot fields stubbed), mem_size, clock_num/clock_den, masked CPUID table slot, skid_margin (default 8192) + resync_slack (1024), epoch_len (default 50_000_000) + hash_epochs (EPOCHS_ON default — API.md §2.1), base image content hash, device set — ARCH §2.2/§8.1 MCFG. NOTE: entropy_seed is NOT a MachineConfig field — it is a per-segment parameter (CreateVm field 2 / DHILOG header, ARCH §5) and must NOT enter the machine_config_hash preimage (the determinism class must not vary per seed). Canonical byte encoding so machine_config_hash is stable (feeds H_0 of the state hash, §8.5). HOST-RUNNABLE (pure types + golden encoding test, any arch). Crate per layout decision (dh-vmm or surviving dh-types). Files: crates/dh-vmm/src/config* (or dh-types/**)" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $MACHINECONFIG $CRATE_LAYOUT

ELF_BOOT=$(bd create "ELF boot path: PT_LOAD loader, identity page tables, direct 64-bit entry, BootInfo" \
  -d "ARCH §2.3 type 1: load ELF PT_LOAD segments into guest RAM, identity-mapped 4-level page tables in low RAM, enter 64-bit directly (CR0/CR4/EFER/GDT via KVM_SET_SREGS), RIP=e_entry, RSI=&BootInfo (versioned struct at fixed GPA). Boot is deterministic: same image + same MachineConfig => same state at any icount. HARDWARE-GATED acceptance (boots device-exercise programs); loader logic unit-testable host-side. Files: crates/dh-vmm/src/boot*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $ELF_BOOT $CLI_BOOT
bd dep add $ELF_BOOT $MACHINECONFIG

MMIO_BUS=$(bd create "dh-devices: DetDevice trait, MmioBus, DevCtx, register convention, lint gate" \
  -d "ARCH §6: DetDevice {mmio_read, mmio_write, snapshot, restore}; DevCtx carries icount, input-log writer, guest memory, interrupt-request queue (drained by boundary engine — devices never inject directly), entropy PRNG. §6.1 register convention: 4 KiB window, 0x00 MAGIC / 0x04 VERSION, 4/8-byte naturally aligned, unaligned => guest fault. Deny-list gate in dh-devices: std::time, rand::*, network types forbidden (clippy + CI grep) — no device reads host time/randomness/IO on the execution path. HOST-RUNNABLE (mock DevCtx unit tests, any arch). Files: crates/dh-devices/src/{lib,bus,ctx}*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $MMIO_BUS $CRATE_LAYOUT

DHILOG_MIN=$(bd create "dh-inputlog minimal Phase-1 writer (versioned DHILOG subset)" \
  -d "Versioned header (entropy_seed 32B, clock rational, base_snapshot_id); canonical records: DEV_EVENT (incl. PIO_ANSWER encoding for detcall IN returns), PAD_SET; AUX records: ENTROPY {icount,len,digest8}, TIMER_FIRE (with delivered_icount), SDK_EVENT (digest), FRAME_MARK (absolute F -> icount). Pure no_std-compatible core, blake3 only dep (ARCH §1). Full codec + golden bytes + fuzz is M5 — version the format so M5 extends without breaking. HOST-RUNNABLE incl. aarch64. Files: crates/dh-inputlog/**" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $DHILOG_MIN $CRATE_LAYOUT

PV_CLOCK=$(bd create "pv-clock device model (0xD000_0000)" \
  -d "ARCH §6.2: VNS_LO/HI and ICOUNT (RO, computed from live counter at the exit — pure function of icount, no log record), TIMER_DEADLINE (RW, write 0 disarms, one-shot), TIMER_VECTOR (default 0x30), FREQ_NUM/DEN (RO). Timer arming hands off to the M3 agenda (interface stub now). DetDevice snapshot/restore of register state. HOST-RUNNABLE unit tests w/ mock DevCtx; integration HARDWARE-GATED. Files: crates/dh-devices/src/clock*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $PV_CLOCK $MMIO_BUS

PV_PAD=$(bd create "pv-pad device model (0xD000_1000): latch, FRAME_COUNTER, frame table" \
  -d "ARCH §6.4: 4-port u32 latch; latch changes ONLY when a canonical PAD_SET record lands at its icount; PAD0..3 RO reads; IRQ_VECTOR optional edge (default disabled); FRAME_COUNTER (RW) — guest MMIO-writes F each emulated frame and that write IS the frame-boundary exit; host records absolute F -> exit icount in the per-segment frame table (mirrored as DHILOG AUX FRAME_MARK). FRAME_MARK consistency rule §6.6: ring-W FrameMark index MUST equal written FRAME_COUNTER, mismatch => guest-contract violation (FAULTED). HOST-RUNNABLE unit; integration HARDWARE-GATED. Files: crates/dh-devices/src/pad*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $PV_PAD $MMIO_BUS
bd dep add $PV_PAD $DHILOG_MIN

PV_ENTROPY=$(bd create "pv-entropy device model (0xD000_2000) + seeded ChaCha20 PRNG" \
  -d "ARCH §6.3 + §5: BUF_GPA/LEN/DOORBELL — on doorbell write, fill LEN bytes at BUF_GPA from the per-VM ChaCha20Rng (rand_chacha), STATUS=1, synchronous (data in place when the MMIO write retires); append AUX ENTROPY {icount,len,digest8}. PRNG seeded from entropy_seed; never reads host entropy. Keep {seed, stream, word_pos} exportable (get_seed/get_stream/get_word_pos) for the M4 ENTR section. HOST-RUNNABLE unit; integration HARDWARE-GATED. Files: crates/dh-devices/src/entropy*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $PV_ENTROPY $MMIO_BUS
bd dep add $PV_ENTROPY $DHILOG_MIN

PV_BLK=$(bd create "pv-blk device model (0xD000_4000): RO base image + CoW overlay" \
  -d "ARCH §6.5: one-deep request regs (SECTOR/BUF_GPA/COUNT/CMD read|write|flush/STATUS), synchronous completion inside the MMIO-write emulation (zero timing variance). Backend: base image O_RDONLY (content hash recorded in MachineConfig) + overlay of 64 KiB clusters in HashMap<cluster_idx, Box<[u8;65536]>> populated on first write (RMW from base); reads check overlay first. Snapshot hook serializes dirty clusters (full codec is M4). Unit test asserts base file bytes + mtime unchanged after writes. HOST-RUNNABLE unit w/ temp files; integration HARDWARE-GATED. Files: crates/dh-devices/src/blk*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $PV_BLK $MMIO_BUS

SERIAL=$(bd create "debug-serial device model: 16550-subset, PIO 0x3F8 + MMIO mirror" \
  -d "ARCH §6.9: output-only (input would be nondeterministic); bytes to the slot's JSON log; RX-register reads return 0; serial output never enters the state hash (disabled for state purposes in all modes). Replaces the minimal M0 sink in dh-cli. HOST-RUNNABLE unit; integration HARDWARE-GATED. Files: crates/dh-devices/src/serial*" \
  -p 1 -l m1-devices -t task --silent)
bd dep add $SERIAL $MMIO_BUS

DETGUEST_DEP=$(bd create "Wire detguest-host path dependency on sibling ../guest-sdk repo" \
  -d "Decision made: detguest-host = { path = \"../guest-sdk/...\" } in workspace deps, same pattern as determinism-proto -> ../control-plane. Verify the guest-sdk Milestone-1 API surface this repo consumes exists: Channel::attach, drain_events, push_command, read_manifest (seqlock retry), read_region, ChannelWriteSink, InjectResponder (guest-sdk IMPLEMENTATION-PLAN Ms1). CI sibling-checkout wiring is owned by the workflow-split bead (avoids concurrent .github/workflows edits). CROSS-REPO: blocked on guest-sdk Ms1 (zero hypervisor dependency, proceeds in parallel) — if the API is missing, file a documentation issue per the clean-room rule, do not improvise from external sources. HOST-RUNNABLE. Files: Cargo.toml" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $DETGUEST_DEP $CRATE_LAYOUT

DETCHANNEL=$(bd create "detchannel host side: PIO detcall handler + doorbell drain + DEV_EVENT logging" \
  -d "ARCH §6.6 (channel spec owned by guest-sdk, read .agents/docs/guest-sdk/ API §3-§5 — never restate normatively): KVM_EXIT_IO handler for 0xD370-0xD39F — IDENT (0xD370), CHANNEL_INIT (0xD374/78/7C: validate + detguest_host::Channel::attach, record channel base GPA, read region manifest seqlock-consistent), DOORBELL (0xD380: drain guest->host rings inside the exit, stamp doorbell icount, AUX SDK_EVENT digests, forward payloads), INJECT (0xD384), QUIESCE_ACK (0xD388, no caller in v1 loop). Every IN return is a canonical DEV_EVENT/PIO_ANSWER record; every host mutation of channel memory goes through ChannelWriteSink as a canonical DEV_EVENT; host touches channel memory ONLY while paused or inside a detcall exit (guest-sdk load-bearing invariant). Rings also drained at every pause boundary. HARDWARE-GATED integration; protocol logic HOST-RUNNABLE over mock GuestMem. Files: crates/dh-devices/src/detchannel*" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $DETCHANNEL $DETGUEST_DEP
bd dep add $DETCHANNEL $MMIO_BUS
bd dep add $DETCHANNEL $DHILOG_MIN

CPUID_MASK=$(bd create "CPUID determinism mask + KVM_PMU_CAP_DISABLE + dh-cli cpuid-diff" \
  -d "ARCH §7.2: start from KVM_GET_SUPPORTED_CPUID, clear RDRAND (leaf1 ECX[30]), RDSEED (leaf7 EBX[18]), TSC_DEADLINE, ARAT, MWAIT/MONITOR, x2APIC, PDCM/PMU, RDTSCP (80000001H:EDX[27]), invariant-TSC advert, TM/turbo/thermal leaves zeroed, KVM paravirt leaves 0x4000_00xx REMOVED entirely; force KVM_CAP_PMU_CAPABILITY/KVM_PMU_CAP_DISABLE (in-guest vPMU off — must not interact with our host counter). Every cleared bit gets a code comment naming the nondeterminism it closes. Masked table goes into MachineConfig and is hashed (determinism class). dh-cli cpuid-diff dumps supported-vs-masked for review (M1 acceptance). HARDWARE-GATED. Files: crates/dh-vmm/src/cpuid*, tools/dh-cli/src/cpuid* (dh-cli scaffold owned by the boot bead — dep below avoids a reservation collision)" \
  -p 0 -l m1-devices -t task --silent)
bd dep add $CPUID_MASK $KVM_FOUNDATION
bd dep add $CPUID_MASK $MACHINECONFIG
bd dep add $CPUID_MASK $CLI_BOOT

MSR_FILTER=$(bd create "Default-deny MSR filter + deterministic MSR emulation" \
  -d "ARCH §2.2: KVM_X86_SET_MSR_FILTER default-deny read+write; allow ranges only EFER, STAR/LSTAR/CSTAR/FMASK, FS/GS base, SYSENTER_*, PAT, TSC_AUX. Everything else exits KVM_EXIT_X86_RDMSR/WRMSR (needs KVM_CAP_X86_USER_SPACE_MSR + KVM_MSR_EXIT_REASON_FILTER, checked in preflight): unknown reads return a fixed config value, logged trace-level; unknown writes => guest #GP (surface, never silently absorb — risk R6). HARDWARE-GATED. Files: crates/dh-vmm/src/msr*" \
  -p 1 -l m1-devices -t task --silent)
bd dep add $MSR_FILTER $KVM_FOUNDATION

NK_DEVICE_PROG=$(bd create "Nanokernel device-exercise program" \
  -d "IMPLEMENTATION-PLAN M1 acceptance program: reads clock, draws entropy, reads pad latch, writes/reads blk sectors, initializes a minimal detchannel page (CHANNEL_INIT detcall) and sends ring-W records via the doorbell detcall; prints progress to debug-serial. Channel page layout per .agents/docs/guest-sdk/ (clean-room: those docs only). HOST-RUNNABLE build; run HARDWARE-GATED. Files: tests/nanokernel/device_exercise*" \
  -p 0 -l nanokernel -t task --silent)
bd dep add $NK_DEVICE_PROG $NK_TOOLCHAIN

BASE_IMAGE=$(bd create "Read-only base image + CoW overlay fixture production" \
  -d "Script/build step producing the pv-blk base disk image (content-hashed into MachineConfig) and overlay fixtures for M1 acceptance: known sector patterns so the guest can verify reads, plus a write set that must land in the overlay only (base byte-unchanged, mtime-checked). HOST-RUNNABLE production; consumption gated. Files: tests/nanokernel/image* (build scripts + fixtures)" \
  -p 1 -l nanokernel -t task --silent)
bd dep add $BASE_IMAGE $NK_TOOLCHAIN

M1_GATE=$(bd create "M1 acceptance: device-exercise integration test, end-to-end" \
  -d "IMPLEMENTATION-PLAN M1 accept, in tests/determinism: boot nanokernel device-exercise program via dh-cli; host asserts every device value; every channel mutation logged as DEV_EVENT; base image file byte-unchanged (mtime-checked); MachineConfig plumbed end to end; cpuid-diff output reviewed/committed. HARDWARE-GATED: run on the Intel box via dh-cli (CI wiring into the kvm-intel job follows once the workflow split lands — not a dependency). Files: tests/determinism/m1_*" \
  -p 0 -l m1-devices -t task --silent)
for parent in $ELF_BOOT $PV_CLOCK $PV_PAD $PV_ENTROPY $PV_BLK $SERIAL $DETCHANNEL $CPUID_MASK $MSR_FILTER $NK_DEVICE_PROG $BASE_IMAGE; do
  bd dep add $M1_GATE $parent
done

# ============================================================
# M2 — detclock: retired-instruction counting + exact landing
# (All M2 acceptance is gated on §7.4 host config: isolcpus/
#  nohz_full/rcu_nocbs, NMI watchdog off, THP off, kvm_intel params.)
# ============================================================

DETCLOCK=$(bd create "dh-detclock: guest-only pinned INST_RETIRED counter" \
  -d "ARCH §3.1: raw perf_event_attr via perf-event-open-sys (NO high-level perf crate — we need guest-only counting + signal-driven overflow): PERF_COUNT_HW_INSTRUCTIONS, pinned=1 (startup fails if not pinned — NMI watchdog must be off per §7.4 or a counter is consumed), exclude_host/hv/idle=1, guest user+kernel counted; reads via mmap'd perf_event_mmap_page with read() fallback, only while vCPU is out of guest mode; PERF_EVENT_IOC_RESET latch at restore (icount segment-relative). FALLBACK (risk R2, document in crate docs): if INST_RETIRED interrupt-retirement empirics fail on this CPU/microcode, switch to retired conditional branches (BR_INST_RETIRED.COND/.NEAR_TAKEN) with the (count, RIP, RCX) boundary tuple — swap contained in dh-detclock + a determinism-class bump. Needs the §7.4-configured box (host-config bead), not the preflight binary. HARDWARE-GATED. Files: crates/dh-detclock/**" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $DETCLOCK $HOSTCFG
bd dep add $DETCLOCK $CRATE_LAYOUT

PMI_KICK=$(bd create "PMI overflow -> immediate-exit kick path" \
  -d "ARCH §3.1: fcntl(F_SETOWN_EX, {F_OWNER_TID, vcpu_tid}) + F_SETSIG(SIGRTMIN+4); signal handler does exactly one thing — set kvm_run.immediate_exit=1 (KVM_CAP_IMMEDIATE_EXIT protocol: signal outside KVM_RUN => next KVM_RUN returns EINTR immediately, no lost wakeups). PERF_EVENT_IOC_PERIOD arming per run segment. HARDWARE-GATED. Files: crates/dh-detclock/**, crates/dh-vmm/src/run*" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $PMI_KICK $DETCLOCK
bd dep add $PMI_KICK $KVM_FOUNDATION

BOUNDARY=$(bd create "Boundary engine: land at exactly icount N (skid margin + single-step)" \
  -d "ARCH §3.2 loop in dh-vmm: if d > SKID_MARGIN(8192)+RESYNC_SLACK(1024) arm PMI at d-SKID_MARGIN and KVM_RUN; else KVM_SET_GUEST_DEBUG single-step; re-read counter after every step (never assume +1); REP rule — never declare a boundary mid-REP, if RIP unchanged keep stepping until RIP advances; instruction exited mid-emulation (MMIO not completed) counts as not-yet-retired; boundary = (icount, RIP), RCX recorded in diagnostics only. Overshoot (c > N) => fatal DivergenceError::Overshoot, a loud DATA_LOSS, never silently absorbed (risk R1). KVM_GUESTDBG_ENABLE dropped the moment the boundary is reached; TF never guest-visible, guest DR writes fault (risk R10). Margins are MachineConfig fields; landed boundary independent of them. Needs only a bootable ELF guest + PMI kick — the milestone-level 'M2 depends on M1' link is carried by the M2 acceptance tests (which dep the M1 gate), so this riskiest-path work is NOT stalled behind the cross-repo detchannel/guest-sdk chain (phase doc: front-load determinism risk). HARDWARE-GATED. Files: crates/dh-vmm/src/boundary*" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $BOUNDARY $PMI_KICK
bd dep add $BOUNDARY $ELF_BOOT

NK_COUNTING_PROG=$(bd create "Nanokernel 1,000-instruction counting-semantics sequence" \
  -d "Known 1,000-instruction sequence including REP MOVS, CPUID, and MMIO-exiting instructions, for the counting_semantics test (IMPLEMENTATION-PLAN M2). Instruction count must be exact by construction (assembled, annotated). HOST-RUNNABLE build; run gated. Files: tests/nanokernel/counting*" \
  -p 1 -l nanokernel -t task --silent)
bd dep add $NK_COUNTING_PROG $NK_TOOLCHAIN

NK_LANDING_PROG=$(bd create "Nanokernel 100M-instruction landing loop program" \
  -d "Deterministic 100M-instruction loop for the M2 landing test and the M3 1e9 determinism regression (loop iterations scale by BootInfo cmdline); touches memory predictably so state hashes are meaningful. P0: gates the P0 landing test and the P0 1e9 determinism regression. HOST-RUNNABLE build; run gated. Files: tests/nanokernel/landing*" \
  -p 0 -l nanokernel -t task --silent)
bd dep add $NK_LANDING_PROG $NK_TOOLCHAIN

COUNTING_TEST=$(bd create "counting_semantics test: single-step 1,000-instruction sequence" \
  -d "IMPLEMENTATION-PLAN M2 accept: single-step the known sequence; counter delta exactly 1,000; REP MOVS retires as 1 on completion; CPUID/HLT/MMIO-exiting instructions retire exactly once on the completing resume (ARCH §3.1). Runs in CI on every host kernel/microcode bump (the R2 empirics test — per-determinism-class assumption, never assumed across classes). On failure: trigger the documented BR_INST_RETIRED fallback decision, do not patch around it. Carries the milestone link: M2 acceptance runs against the completed M1 device surface. HARDWARE-GATED (kvm-intel). Files: tests/determinism/counting*" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $COUNTING_TEST $BOUNDARY
bd dep add $COUNTING_TEST $NK_COUNTING_PROG
bd dep add $COUNTING_TEST $M1_GATE

LANDING_TEST=$(bd create "Landing-precision test: 10,000 random targets, zero overshoots" \
  -d "IMPLEMENTATION-PLAN M2 accept: for 10,000 random targets N in the 100M-instruction loop, stop at exactly N (icount == N, RIP at instruction start) with ZERO overshoots, including targets that land across REP-string instruction boundaries. Carries the milestone link: M2 acceptance runs against the completed M1 device surface. HARDWARE-GATED (kvm-intel, §7.4-configured host). Files: tests/determinism/landing*" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $LANDING_TEST $BOUNDARY
bd dep add $LANDING_TEST $NK_LANDING_PROG
bd dep add $LANDING_TEST $M1_GATE

SKID_HARNESS=$(bd create "Skid-histogram harness in dh-verify + dh-cli, margin/2 assertion" \
  -d "Exit-gate tooling with a named home (ARCH §1): dh-verify collection + a dh-cli subcommand. Measure PMI skid distribution over the landing runs, export the histogram (artifact + Prometheus-ready series per ARCH §9 'PMI skid histogram'); assert measured max skid on the box < skid_margin/2, else raise margin and re-baseline (risk R1 alert threshold). HARDWARE-GATED, quiesced host. Files: crates/dh-verify/src/skid*, tools/dh-cli/src/skid*" \
  -p 0 -l m2-detclock -t task --silent)
bd dep add $SKID_HARNESS $BOUNDARY
bd dep add $SKID_HARNESS $NK_LANDING_PROG

# ============================================================
# M3 — virtual time, deterministic injection, run control
# (Host-runnable math seams below can start before M2 lands.)
# ============================================================

VNS_MATH=$(bd create "Virtual-time rational math: vns <-> icount conversions" \
  -d "ARCH §4: vns = icount * clock_num / clock_den (u128 intermediate, truncating); timer conversion T -> target icount = ceil(T * clock_den / clock_num); default 1:1 ('deterministic 1 GHz'); rational fixed per VM lifetime, recorded in every DHILOG header. Frames are not time — no frame-period arithmetic anywhere. Property tests for overflow/rounding/monotonicity. P0: on the critical path to run control. HOST-RUNNABLE on any arch incl. aarch64 (unit layer; EARLY-START seam — no M2 dependency). Files: crates/dh-vmm/src/vt*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $VNS_MATH $CRATE_LAYOUT

AGENDA=$(bd create "Agenda/scheduler: compile Run(until) into sorted stop points" \
  -d "ARCH §3.3: merge scheduled injections (icount each), epoch boundaries (every epoch_len), goal polls (goal_poll_period, only if until has a goal), final stop (icount budget / vns budget converted). Every agenda point is a pure function of the inputs => two replays compute identical agendas (property-tested). Dynamic stops (next_sdk_event doorbell check, frame_budget Nth frame-boundary exit) are flags consumed by run control, not agenda entries. P0: on the critical path to run control. HOST-RUNNABLE pure logic on any arch (EARLY-START seam — can land before M2). Files: crates/dh-vmm/src/agenda*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $AGENDA $VNS_MATH

STATE_HASH=$(bd create "Minimal Phase-1 final-state hash (owner: dh-vmm StateHashChain)" \
  -d "Scoped ARCH §8.5 for M3's run-twice-compare (full chain + XSAVE canonicalization is M4, excluded by the sequencing guard): H_0 = blake3('dh-statehash-v1' || machine_config_hash || base_snapshot_ref); final link over canonical vCPU bytes (KVM_GET_REGS/SREGS2/FPU/VCPU_EVENTS/DEBUGREGS + explicit MSR list — the full §8.1 non-XSAVE subset; document that XSAVE canonicalization is deferred to M4), device sections (via DetDevice::snapshot), ALL guest RAM pages ascending idx (le64(page_idx) || 4096 bytes — full-memory walk; dirty-ring delta arrives in M4), then le64(icount) || le64(vns). Same harvest order as §8.5 so M4 extends rather than replaces. Hash logic HOST-RUNNABLE; end-to-end HARDWARE-GATED. Files: crates/dh-vmm/src/hash*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $STATE_HASH $ELF_BOOT
bd dep add $STATE_HASH $MACHINECONFIG
bd dep add $STATE_HASH $MMIO_BUS

INJECTION=$(bd create "Deterministic interrupt injection rule (IF=0 deferral)" \
  -d "ARCH §3.4: land at B; injectable iff kvm_run.ready_for_interrupt_injection==1 && if_flag==1 && no pending exception (KVM_GET_VCPU_EVENTS); if yes KVM_INTERRUPT(v) delivered before any guest instruction retires; if not, set request_interrupt_window=1 and single-step forward, re-checking each step, deliver at the FIRST injectable boundary >= B — injectability is a pure function of guest state so every replay defers identically. Record actual delivery boundary in AUX TIMER_FIRE.delivered_icount. HARDWARE-GATED. Files: crates/dh-vmm/src/inject*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $INJECTION $BOUNDARY

TIMER_ARM=$(bd create "pv-clock timer arming -> agenda point -> vector injection" \
  -d "ARCH §4 timers: TIMER_DEADLINE write converts T -> target icount (ceil rule), inserts an agenda point, injects TIMER_VECTOR per §3.4 at that boundary; write 0 disarms; one-shot. Guest-armed so deterministic; logged as AUX TIMER_FIRE for verification. HARDWARE-GATED integration; conversion + agenda insertion HOST-RUNNABLE. Files: crates/dh-devices/src/clock*, crates/dh-vmm/src/agenda*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $TIMER_ARM $PV_CLOCK
bd dep add $TIMER_ARM $AGENDA
bd dep add $TIMER_ARM $INJECTION

RUN_CONTROL=$(bd create "Run(until ...) semantics + Pause epoch-boundary roll-forward + dh-cli run" \
  -d "ARCH §3.3 + IMPLEMENTATION-PLAN M3: until = icount budget | vns budget | goal (with goal polling) | next_sdk_event{stream?} (doorbell detcall exit checks stop condition; non-matching events forwarded without stopping) | frame_budget (Nth pv-pad FRAME_COUNTER frame-boundary exit since run start — the only frame-quantized stop). Asynchronous Pause: honored at next exit of any kind, then ROLL FORWARD to the next epoch boundary before reporting paused (pause-soon-at-deterministic-point, latency <= epoch_len; document). Driven via dh-cli run for Phase 1 (full gRPC Run RPC is M6; API.md §2.4 is the eventual shape — keep the until enum aligned). HARDWARE-GATED end-to-end; agenda interplay unit-tested host-side. Files: crates/dh-vmm/src/run*, tools/dh-cli/src/run*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $RUN_CONTROL $AGENDA
bd dep add $RUN_CONTROL $INJECTION
bd dep add $RUN_CONTROL $DETCHANNEL
bd dep add $RUN_CONTROL $PV_PAD

NK_TIMER_PROG=$(bd create "Nanokernel timer-arming program (+ IF=0 masked-window variant)" \
  -d "IMPLEMENTATION-PLAN M3 accept programs: arms a pv-clock timer every 1ms-vns for 10s-vns, ISR records delivered icounts to a guest-RAM table the host reads; variant (cmdline flag) runs with interrupts masked (IF=0) across timer deadlines for the deferral test. HOST-RUNNABLE build; run gated. Files: tests/nanokernel/timer*" \
  -p 1 -l nanokernel -t task --silent)
bd dep add $NK_TIMER_PROG $NK_TOOLCHAIN

TIMER_DETERMINISM=$(bd create "M3 timer determinism test: identical delivered icounts across 100 runs" \
  -d "IMPLEMENTATION-PLAN M3 accept: nanokernel arms a timer every 1ms-vns for 10s-vns; the delivered-icount list is identical across 100 repeated runs (exact-match list compare). HARDWARE-GATED (kvm-intel). Files: tests/determinism/timer*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $TIMER_DETERMINISM $TIMER_ARM
bd dep add $TIMER_DETERMINISM $RUN_CONTROL
bd dep add $TIMER_DETERMINISM $NK_TIMER_PROG

IF0_TEST=$(bd create "IF=0 deferral test: masked timer defers identically across runs" \
  -d "IMPLEMENTATION-PLAN M3 accept: timer lands while the guest has interrupts masked; delivery defers to the first injectable boundary >= B identically across runs (compare TIMER_FIRE.delivered_icount lists; delivered > requested). HARDWARE-GATED (kvm-intel). Files: tests/determinism/if0*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $IF0_TEST $TIMER_ARM
bd dep add $IF0_TEST $NK_TIMER_PROG
bd dep add $IF0_TEST $RUN_CONTROL

TSC_BENCH=$(bd create "Guest-TSC alignment benchmark: per-entry MSR write vs TSC offset attribute" \
  -d "ARCH §4 defense 4 caveat + IMPLEMENTATION-PLAN M3 accept: benchmark per-entry KVM_SET_MSRS{IA32_TSC} value writes (can engage KVM's TSC-sync heuristics; costs at ~3k exits/guest-s) vs the KVM_VCPU_TSC_CTRL offset attribute. Produce measured numbers on the box and PICK ONE, recorded in docs, BEFORE M4 freezes restore behavior (restore writes IA32_TSC <- vns). HARDWARE-GATED. Files: crates/dh-vmm/src/run*, docs/**" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $TSC_BENCH $RUN_CONTROL

DET_REGRESSION=$(bd create "CI determinism regression: 1e9 instructions twice, equal final hash" \
  -d "IMPLEMENTATION-PLAN M3 accept: run nanokernel (landing-loop program scaled to 1e9) twice from cold boot with fixed seed; final state hash equal. Lands in tests/determinism + the kvm-intel workflow; becomes required-for-merge from M3 onward (wiring bead below). Any mismatch is P0 (MAP.md conventions). HARDWARE-GATED (kvm-intel). Files: tests/determinism/regression*, .github/workflows/** (workflow edits land after the split bead, which this depends on)" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $DET_REGRESSION $RUN_CONTROL
bd dep add $DET_REGRESSION $STATE_HASH
bd dep add $DET_REGRESSION $NK_LANDING_PROG
bd dep add $DET_REGRESSION $CI_SPLIT

CI_REQUIRED=$(bd create "Make determinism regression required-for-merge (branch protection)" \
  -d "Wire the M3 run-twice-compare job as a required status check from M3 onward (IMPLEMENTATION-PLAN M3); nightly adds the determinism-class.lock drift check. Document the policy in CONTRIBUTING/README. HARDWARE-GATED job; settings task. Files: .github/workflows/**, repo settings" \
  -p 0 -l determinism-ci -t task --silent)
bd dep add $CI_REQUIRED $DET_REGRESSION
bd dep add $CI_REQUIRED $CI_LOCK

GATE_100=$(bd create "Phase-1 determinism gate harness: 100-run zero-divergence, in dh-verify" \
  -d "Exit-gate tooling with a named home (phase doc gate 1; ARCH §1): dh-verify + a dh-cli subcommand. One command: boot nanokernel, run to icount N twice -> compare chained state hash; repeat with an injected timer event at an exact icount -> compare; 100 consecutive runs, zero divergence; emits a report artifact (run list, hashes, pass/fail). HARDWARE-GATED (kvm-intel, §7.4 host). Files: crates/dh-verify/src/gate*, tools/dh-cli/src/gate*" \
  -p 0 -l m3-runctl -t task --silent)
bd dep add $GATE_100 $STATE_HASH
bd dep add $GATE_100 $RUN_CONTROL
bd dep add $GATE_100 $TIMER_ARM
bd dep add $GATE_100 $NK_LANDING_PROG
bd dep add $GATE_100 $NK_TIMER_PROG

PHASE1_EXIT=$(bd create "Phase 1 exit gate sign-off — sequencing guard before M4" \
  -d "Verify and record: determinism gate 100/100 zero divergence; landing gate 10,000 targets exact with zero overshoots incl. REP boundaries; max skid < skid_margin/2 (histogram archived); counting_semantics green; M3 accepts green (timer 100-run, IF=0 deferral); CI determinism regression required-for-merge and green; TSC alignment decision recorded with measured numbers. SEQUENCING GUARD (phase doc): hypervisor M4 (snapshots) MUST NOT start until this closes — snapshotting a nondeterministic VM produces unfalsifiable bugs. HARDWARE-GATED review. Files: docs/**" \
  -p 0 -l m3-runctl -t task --silent)
for parent in $GATE_100 $LANDING_TEST $SKID_HARNESS $COUNTING_TEST $TIMER_DETERMINISM $IF0_TEST $DET_REGRESSION $CI_REQUIRED $TSC_BENCH; do
  bd dep add $PHASE1_EXIT $parent
done

# ============================================================
# Docs
# ============================================================

DOC_PARTITION=$(bd create "Document test partitioning + Intel-box runbook" \
  -d "Write docs: which tests/gates are host-runnable (any machine incl. macOS/aarch64) vs kvm-intel-gated, how an agent runs each locally; Intel-box runbook (apply §7.4 config, run preflight, run gates via dh-cli); cross-reference the determinism-class re-baseline procedure. HOST-RUNNABLE. Files: docs/**" \
  -p 2 -l docs -t task --silent)
bd dep add $DOC_PARTITION $CI_SPLIT
bd dep add $DOC_PARTITION $CI_LOCK

DOC_ASBUILT=$(bd create "Phase-1 as-built docs: crate layout decisions, dh-cli usage, R2 status" \
  -d "Update README/docs to as-built: final crate dispositions (dh-types/dh-kvm/dh-smoke), dh-cli subcommands (boot, run, cpuid-diff, gate/skid harnesses), measured skid + TSC benchmark numbers, INST_RETIRED empirics result and fallback status (R2). HOST-RUNNABLE. Files: README.md, docs/**" \
  -p 2 -l docs -t task --silent)
bd dep add $DOC_ASBUILT $PHASE1_EXIT

echo ""
echo "Bead graph created!"
echo "  bd ready              # unblocked tasks (expect: crate layout, host config, CI runner...)"
echo "  bd dep tree           # full dependency tree"
echo "  bd dep cycles         # must be empty"
echo "  bd list -l m2-detclock  # per-milestone view"
