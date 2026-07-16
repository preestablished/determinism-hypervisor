# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- **OSFXSR/SSE for future compiled-language guests (S1).** `enter_long_mode`
  (`crates/dh-vmm/src/boot.rs:233`) sets `cr4 = 1 << 5` (PAE only). With
  CR4.OSFXSR clear, any SSE instruction faults #UD → (no IDT) triple fault.
  Current asm nanokernel guests emit no SSE, so this is a non-issue now, but
  the first Rust/compiled freestanding guest (`reference-workload`, M1/M7)
  will emit XMM and mysteriously triple-fault. *Action:* file a follow-up
  bead to set `CR4.OSFXSR` (likely `OSXSAVE` + `XCR0` too) before the first
  compiled guest, and add a one-line `enter_long_mode` comment noting "PAE
  only — SSE-using guests need OSFXSR (future loader work)".

- **Explicit upper bound in `load_elf` (S2).** `load_elf` takes only `mem`,
  not `mem_bytes`; an oversized `p_vaddr`/`p_memsz` is caught only by
  `write_slice` returning a `GuestMemoryError`. That fails safe (no
  corruption — `vm_memory` write is bounds-checked), so this is
  clarity/robustness, not a bug. *Action:* either pass `mem_bytes` and reject
  `p_vaddr + p_memsz > mem_bytes` with a precise `BootError::Elf`, or add a
  comment that the upper bound is delegated to `write_slice` (the lower bound
  `< LOW_RAM_RESERVED` is already explicit, so the asymmetry is confusing).

- **`run_until_hlt` signature smell (S3).** `tools/dh-cli/src/boot.rs`
  `run_until_hlt(mut slot: SlotVm, ...)` takes the whole slot by value + `mut`
  but only mutates the vcpu. *Action (optional):* take `&mut slot.vcpu` or
  `&mut SlotVm` for clarity. Cosmetic.

- **ELF phdr sanity caps (S4).** No `phentsize >= 56` / `phnum` cap. Inputs
  are trusted in-tree images so there is no exposure today; failures are safe
  (`truncated phdr`). *Action (only if untrusted ELFs ever loaded):* add a
  `phentsize` lower bound and a `phnum` cap.

### Doc nit (not a code change to this iteration's source)

- **Commit-message "every lane" claim is inaccurate for the arm lane.** The
  iteration-31 commit body says the 3 host-side unit tests "run in EVERY lane
  — no /dev/kvm needed". They need no KVM, but `.github/workflows/ci.yaml`
  runs the arm host lane with `--workspace --exclude dh-vmm --exclude
  dh-worker --exclude dh-cli`, so dh-vmm's `boot::` tests do **not** run on
  arm at all. They run on the x86 hosted lane and the kvm-intel lane. *Action:*
  in future commit prose, say "every x86 lane" (or "both x86 lanes"), not
  "every lane". No source change required.
