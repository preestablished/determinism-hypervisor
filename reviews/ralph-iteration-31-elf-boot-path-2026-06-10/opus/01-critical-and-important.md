# Critical and Important findings

**None.**

No correctness, safety, or determinism defects were found in this branch. The two
behaviors the iteration is responsible for — mapping the MMIO hole into guest page
tables and applying the default-deny MSR filter at boot — are implemented correctly and
proven by a live KVM test that reaches an MMIO exit at `0xD000_0008` instead of
triple-faulting.

The points the review brief asked to scrutinize all resolved cleanly:

1. **Page-table slot addressing** (`pd + ((gpa % GIB) / PAGE_2M) * 8`) — verified
   independently. The MMIO hole lands in PD #3, slot 128 (PTE GPA `0x6400`); at the
   maximum legal RAM size (`mem_bytes == MMIO_HOLE_BASE == 0xD000_0000`) the highest RAM
   2 MiB page is `0xCFE0_0000`, which lands in PD #3 slot 127 — adjacent to the hole PTE
   with no collision. All four PD pages (`0x3000..0x7000`) and the highest written PTE
   (`0x6FF8`) stay strictly below the BootInfo page at `0x7000`. The always-four PDPT
   entries are correct: unused PD pages are zeroed guest RAM, so an unmapped region
   page-walks to a not-present entry and #PFs only if the guest actually touches it.

2. **Determinism** — `load_and_enter` and all callees read no host state (no env, time,
   RNG, or `/proc`). Identical `(elf, MachineConfig, cmdline)` produce byte-identical
   guest RAM: the page tables, BootInfo, and PT_LOAD copies are pure functions of the
   inputs, fresh guest RAM is zeroed, and the BootInfo page's trailing bytes past the
   cmdline remain zeroed RAM (the loader writes only `0x20 + cmdline.len()` bytes).

3. **`0x5000 → 0x7000` BootInfo move** — no consumer hardcodes the old address. The
   guest receives the BootInfo GPA in `RSI`; `tests/nanokernel` references only relative
   field offsets (`BOOTINFO_OFF_*`), which match the impl exactly. `BOOTINFO_GPA` is
   exported from the module.

4. **MSR resume contract** — the dh-cli loop's bare `continue` on
   `MsrReadDenied`/`MsrWriteDenied` is correct because `classify_exit`
   (`crates/dh-vmm/src/kvm.rs:338-357`) writes the deterministic reply into the
   `kvm_run` MSR buffer (`*exit.data`/`*exit.error`) *before* returning the event, so the
   value is already staged for `KVM_RUN` re-entry.

5. **MSR filter idempotency / double-apply** — `apply_default_deny_filter` is applied once
   per VM in `load_and_enter`; the dh-vmm `msr` unit tests apply it on their own separate
   VMs. No same-VM double-apply.

6. **`create_slot_vm` cap** — `crates/dh-vmm/src/kvm.rs:122` rejects
   `mem_bytes > MMIO_HOLE_BASE`, which is what makes the "RAM and hole both fit in the
   four PDs without collision" invariant hold. The cap is enforced and tested
   (`mem_above_hole_rejected`).
