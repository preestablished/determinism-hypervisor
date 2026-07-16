# Critical & Important Findings

**None.**

No Critical or Important issues were found. The stub is functionally correct (verified by
disassembly and a green test run) and its one notable deviation — skipping the real-mode
transition — is justified below rather than being a defect.

---

## Title-vs-description judgment call (resolved: deviation is correct and documented)

The bead title names a *"real-mode→long-mode stub"*, and IMPLEMENTATION-PLAN.md M0 echoes
it verbatim:

> **IMPLEMENTATION-PLAN.md M0 (line 18):**
> "real-mode→long-mode stub that writes to debug-serial and HLTs."

But that wording is internally inconsistent with the project's own boot architecture.
ARCHITECTURE.md §2.3 specifies that freestanding-ELF guests are entered in long mode
**directly**, with no real-mode phase in the guest at all:

> **ARCHITECTURE.md §2.3:**
> "`dh-vmm` loads the ELF PT_LOAD segments into guest RAM, sets up identity-mapped
> 4-level page tables in low RAM, **enters 64-bit mode directly (CR0/CR4/EFER/GDT set via
> `KVM_SET_SREGS`), `RIP = e_entry`, `RSI = &BootInfo`** ..."

The real-mode→long-mode transition is the **VMM's** responsibility (done host-side via
`KVM_SET_SREGS` before the first `KVM_RUN`), not the guest's. A guest ELF entered at
`e_entry` in long mode has nothing real-mode to write. The plan's "real-mode→long-mode
stub" phrasing describes the *boot path as a whole*, not code the guest contains. The same
M0 section (line 20) corroborates the actual acceptance criterion:

> "`dh-cli boot tests/nanokernel/hello.elf` prints ..."

— i.e. M0 is accepted by *booting `hello.elf` and reading the serial log*, which is exactly
what this stub enables.

The author made the right call: implemented the bead **description** (~20-line serial print
+ HLT park) and documented the discrepancy directly in the asm header
(`hello.asm:5-7`):

> "Note vs the bead title: there is no real-mode→long-mode phase to write — ARCH §2.3's
> ELF boot path enters long mode directly ... so the 'stub' reduces to the print."

This is the correct resolution. The deviation honors M0's *intent* (a bootable guest that
prints to serial and HLTs, accepted via `dh-cli boot`), and it is transparently recorded
where a future reader will see it. **No action required** beyond the optional doc-fix noted
in `02-suggestions.md` (the stale plan wording is worth a one-line correction so the next
agent isn't tripped by the same contradiction).
