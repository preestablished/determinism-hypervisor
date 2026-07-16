# Suggestions (non-blocking)

## S1 — CR4 lacks OSFXSR/OSXSAVE: future non-asm guests executing SSE will #UD

`enter_long_mode` (`crates/dh-vmm/src/boot.rs:233`) sets `cr4 = 1 << 5`
(PAE only). With OSFXSR (CR4.9) clear, any SSE instruction faults #UD; with
no IDT a nanokernel guest triple-faults. The current nanokernel guests are
hand-written asm that emit zero SSE, so this is a **non-issue today**. But
the loader is "the real ELF boot path" intended for `reference-workload` and
future Rust/compiled freestanding guests, where the codegen *will* emit SSE
(x86_64 ABI passes floats in XMM; LLVM uses SSE for memcpy/memset). When
that guest lands it will mysteriously triple-fault at the first XMM touch.

Recommendation: file a follow-up bead to set `CR4.OSFXSR` (and likely
`OSXSAVE` + an `XCR0` write for AVX) before the first compiled-language
guest. Not needed for s0p's asm guests; flag it so it is not a surprise in
the M1/M7 guest beads. A one-line doc comment in `enter_long_mode` noting
"PAE only — SSE-using guests need OSFXSR (future loader work)" would record
the intent.

## S2 — `load_elf` has no explicit upper bound on `p_vaddr` / `p_memsz`

`load_elf` takes only `mem` (not `mem_bytes`), so a PT_LOAD whose
`p_vaddr`/`p_memsz` exceeds guest RAM is caught only by `write_slice`
returning a `vm_memory::GuestMemoryError` (→ `BootError::Mem`). That is a
**safe** failure — no host-memory corruption, since `vm_memory` write_slice
is bounds-checked — so this is robustness/clarity, not a bug. Consider
either passing `mem_bytes` and rejecting `p_vaddr + p_memsz > mem_bytes`
with a precise `BootError::Elf("PT_LOAD beyond guest RAM")`, or adding a
one-line comment that the upper bound is delegated to `write_slice`. The
lower bound (`< LOW_RAM_RESERVED`) is already explicit and tested; the
asymmetry invites a future reader to assume an upper check exists.

## S3 — `run_until_hlt(mut slot, ...)` signature smell

`tools/dh-cli/src/boot.rs:run_until_hlt` takes `mut slot:
dh_vmm::kvm::SlotVm` but only uses `slot.vcpu` mutably (via `&mut` for
`.run()`). Taking the whole `SlotVm` by value + `mut` reads as "this
consumes/mutates the slot" when it really just needs the vcpu for the run
loop. Minor — passing `&mut slot.vcpu` (or `&mut SlotVm`) would be clearer
and would let the caller keep the slot. Pre-existing pattern; cosmetic.

## S4 — `load_elf` does not reject `phnum * phentsize` arithmetic overflow / absurd phnum

The phdr loop computes `at = phoff + i * phentsize` and slices `elf` from
`at`. Out-of-range slices return `None` → `BootError::Elf("truncated
phdr")`, so a malformed `phnum`/`phoff` fails safe. But a hostile
`phentsize` of 0 would make `at` constant and re-read the same phdr `phnum`
times (harmless: same segment copied repeatedly), and very large `phnum`
just iterates until a slice miss. No security exposure (inputs are trusted
guest images built in-tree), but if the loader ever consumes untrusted ELFs
a `phentsize >= 56` sanity check and a `phnum` cap would be worth adding.
