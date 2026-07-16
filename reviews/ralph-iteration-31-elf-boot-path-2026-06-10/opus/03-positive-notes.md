# Positive notes

- **Clean extraction with a single integration point.** `tools/dh-cli/src/boot.rs` sheds
  ~170 lines of duplicated loader/page-table/sregs code and now calls one function,
  `dh_vmm::boot::load_and_enter(&slot, elf, cmdline)`. The dh-cli error enum collapses
  `Elf`/`Mem` into a single `Loader(dh_vmm::boot::BootError)`, so the debug run loop keeps
  only the concerns that are genuinely dh-cli's (serial sink, IN-fill, HLT/budget). Good
  separation: dh-vmm owns "how to load and enter", dh-cli owns "how to drive the run".

- **The MMIO-hole obligation is proven, not asserted.** The new live test
  `device_exercise_reaches_a_real_mmio_exit` boots the real device-exercise guest on
  /dev/kvm and asserts the *first* pv-clock read surfaces as an MMIO exit at
  `0xD000_0008` — exactly the failure mode (triple fault) the old M0 loader produced.
  This is the right kind of test for this iteration: it pins the behavioral delta, not the
  bytes.

- **Explicit bss zero-fill is regression-proofed.** `load_elf` zero-fills `[filesz, memsz)`
  explicitly even though fresh guest RAM is already zeroed, and the unit test
  *pre-dirties* the bss range with `0xFF` before loading to prove the fill actually runs.
  This guards against a future memslot-reuse change silently reintroducing stale bytes.

- **Determinism is designed in and documented.** The module header states the contract
  ("nothing here reads host state"), and the implementation honors it: no env/time/RNG
  reads, BootInfo trailing bytes left as zeroed RAM, page tables and segment caches are
  pure functions of the inputs.

- **The MSR-filter-at-boot + resume contract is coherent end to end.** The filter is
  applied once per VM in `load_and_enter`; `classify_exit` stages the deterministic
  RDMSR/WRMSR reply into the `kvm_run` buffer before returning; the dh-cli loop's
  `continue` on the denied arms is therefore correct, and the comment explaining why
  (no post-classify fill is possible) is accurate.

- **Good defensive ELF parsing.** Magic/class/data/`ET_EXEC`/`EM_X86_64` are all checked;
  `p_offset + p_filesz` uses `checked_add`; the low-RAM reserve (`p_vaddr < 0x8000`)
  rejection protects the loader's own page-table/BootInfo structures; `no PT_LOAD` is
  rejected. Closures `u16le`/`u64le` return `Option` so truncation yields a clean `Err`,
  not a panic.

- **Quality gates are green on this box:** 3 boot unit tests, 4 dh-cli live tests, the
  full 48-test dh-vmm suite, `clippy -D warnings`, and `fmt --check` all pass. The
  amended-in clippy operator-parens fixes hold.
