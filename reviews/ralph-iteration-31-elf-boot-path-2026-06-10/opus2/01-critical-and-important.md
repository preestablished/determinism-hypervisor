# Critical & Important Findings

**None.**

I probed every adversarial angle in the brief and found no correctness,
determinism, or safety defect that rises to Critical or Important. The
items below document the *negative results* — the things that could have
been bugs and are not — so the next reviewer/iteration does not re-litigate
them.

## Cleared: `classify_exit` completes MSR emulation before returning the event

The concern: if `classify_exit` returns `MsrReadDenied`/`MsrWriteDenied`
*without* mutating the `kvm_run` MSR exit object, resuming KVM_RUN would
silently corrupt (stale data) or silently absorb a denied write.

`crates/dh-vmm/src/kvm.rs` `VcpuExit::X86Rdmsr(exit)` / `X86Wrmsr(exit)`
arms set `*exit.data = v; *exit.error = 0;` (or `*exit.error = 1` for the
#GP path) **in place on the exit object, before** the `ExitEvent` is
constructed. The dh-cli debug loop's `MsrReadDenied | MsrWriteDenied => {}`
arm therefore correctly does nothing but resume — the emulation is already
applied. The `msr::denied_rdmsr_exits_to_userspace` and
`denied_wrmsr_injects_gp` live tests pin exactly this (RDMSR → EDX:EAX=0
then HLT; WRMSR → fault, never HLT). **Resume is safe.** ✓

## Cleared: 0x5000 → 0x7000 BootInfo move is fully consistent

- Guest crt0 (`tests/nanokernel/asm/crt0.asm`) reads the pointer from `RSI`
  (`mov [BOOT_INFO_PTR], rsi`) — it is GPA-agnostic, so the move cannot
  break any guest. ✓
- A workspace-wide grep for `0x5000` finds hits **only** in the historical
  iter-29 review files (`reviews/.../opus*/`), never in current source,
  guest asm, docs, or beads. No live code or doc asserts `0x5000`. ✓
- The new layout has no overlap: PDs `0x3000..0x7000`, BootInfo
  `0x7000..0x8000`, `LOW_RAM_RESERVED = 0x8000` guards the guest image. ✓

## Cleared: MMIO-hole PTE never collides with a RAM PTE

`create_slot_vm` rejects `mem_bytes > MMIO_HOLE_BASE`, so the largest legal
RAM is exactly `0xD000_0000`. Its top 2 MiB page maps GPA `0xcfe00000`
(PTE slot `0x63f8`); the hole PTE is at slot `0x6400` (PD#3, index 128).
Adjacent, never the same slot — the hole PTE is never overwritten by the
RAM loop, and no RAM PTE lands at or above the hole. ✓

## Cleared: triple-fault-on-unmapped-GPA is deterministic

PDPT entries for GiB 1..3 point at PD pages that are zeroed except where
the RAM loop / hole loop set entries. A guest touching an unmapped GPA
(or a VA ≥ 512 GiB, since PML4 has only entry 0) takes #PF; with no IDT a
nanokernel guest escalates to a triple fault → `VcpuExit::Shutdown` →
`ExitEvent::Shutdown`, surfaced as an error. This is host-state-free and
deterministic. The dh-cli debug loop turns it into a clean
`UnexpectedExit`. ✓

## Cleared: no cross-subsystem interference at boot

`load_and_enter` calls the loader, then `apply_default_deny_filter` (the
msr.rs "call once after create_slot_vm" contract is satisfied — the vCPU
and USER_SPACE_MSR cap already exist from `create_slot_vm`), then
`enter_long_mode`. Running the dh-vmm boot/msr/cpuid/kvm live tests
together (`cargo test -p dh-vmm`) and the full workspace suite passes
48/48 in dh-vmm with the MSR filter, CPUID determinism mask, and PMU
disable all active. No ordering or state interference observed. ✓
