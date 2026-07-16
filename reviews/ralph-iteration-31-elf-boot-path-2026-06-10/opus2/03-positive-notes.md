# Positive Notes

## Clean extraction of the loader into `dh-vmm`, dh-cli reduced to a driver

The diff moves ~145 lines of duplicated loader logic out of
`tools/dh-cli/src/boot.rs` and into the canonical `crates/dh-vmm/src/boot.rs`,
leaving dh-cli with just the debug run loop. The dh-cli `BootError` now wraps
`dh_vmm::boot::BootError` (`Loader(...)`) instead of carrying its own
`Elf`/`Mem` variants — single source of truth, no drift risk. This is the
right direction: the real boot path lives in the VMM crate, the CLI is a thin
consumer.

## The two iter-29 obligations are genuinely discharged and pinned by a live test

The new `device_exercise_reaches_a_real_mmio_exit` test proves the headline
improvement over the M0 loader: the device guest's pv-clock read now surfaces
as `MMIO at 0xd0000008` instead of page-faulting into a triple fault. I
reproduced this through the actual CLI binary, not just the test harness. The
MMIO-hole PTE (`present|rw|ps` at GPA `0xD000_0000`) is what makes a no-memslot
region produce `KVM_EXIT_MMIO` rather than `#PF` — exactly the §2.2 contract.
The MSR default-deny filter is now applied *inside* `load_and_enter`, so denied
MSR accesses are deterministically emulated from the first instruction.

## Determinism holds end-to-end through the real binary

Two consecutive `dh-cli boot landing_loop --cmdline 7777` runs produced
byte-identical JSON (`{"serial":"L","exits":2}`). The loader reads no host
state (the module header's claim), and this is observable at the CLI layer,
not merely asserted in a unit test.

## Page-table construction is index-correct and self-checking

`write_page_tables` derives each PTE slot from the GPA
(`PD_BASE + (gpa/GIB)*0x1000` then `((gpa%GIB)/PAGE_2M)*8`) rather than a
running counter, so it is robust to the four-PD split. The unit test asserts
the first and last RAM PTEs *and* the hole PTE byte value — I confirmed the
hole lands in PD#3 at slot index 128, one slot above the highest possible RAM
PTE, with no collision.

## The hand-built test ELF is spec-accurate

`elf_loader_copies_and_zero_fills` constructs a minimal ELF64 by hand with
correct field offsets (phentsize 56, p_offset@8, p_vaddr@16, p_filesz@32,
p_memsz@40; e_phentsize@54, e_phnum@56), pre-dirties the bss range with
`0xFF`, and asserts both the copied bytes and the zero-fill — a real proof of
the `[filesz, memsz)` zero-fill contract, plus rejection of a low-RAM-overlap
segment and a truncated file. This is exactly the kind of test that catches a
future off-by-one in the loader.

## Honest, accurate module documentation

The `boot.rs` header spells out the low-RAM layout, the determinism posture
("nothing here reads host state"), and the precise obligations beyond M0. The
dh-cli header is candid that the debug loop answers every IN as zeros (so an
LSR-polling 16550 driver would spin) and points at the M1 acceptance bead for
the real device bus. The comments match the code I read.
