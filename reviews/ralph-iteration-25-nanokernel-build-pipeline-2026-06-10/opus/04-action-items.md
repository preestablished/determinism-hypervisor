# Action Items

Each item is self-contained: file, what to do, and why.

### Critical

None.

---

### Important

- [ ] **Make `which()` match an executable file, not any file.**
  `tests/nanokernel/build.rs` `which()` filters on `p.is_file()`, which accepts a
  non-executable regular file named `nasm`/`ld`/`lld` on PATH and turns a clean
  fall-through into a spawn panic. Add a Unix executable-bit check
  (`m.permissions().mode() & 0o111 != 0`). (I-2)

- [ ] **Harden the CI nasm step's environment assumptions.**
  `.github/workflows/ci.yaml:58` assumes passwordless `sudo` + apt on both host lanes.
  Add a comment asserting both lanes are GitHub-hosted Ubuntu, or gate on
  `command -v apt-get`. Separately, the `self-hosted kvm-intel` lane (line 80) has **no**
  nasm step — add the same `which nasm ||` guard there or a comment pointing at the box's
  provisioning manifest, so a reprovisioned box can't silently break the gated build. (I-3)

- [ ] **Add an orphan-section landing zone / W^X note to `link.ld`.**
  `tests/nanokernel/link.ld` handles `.text/.rodata/.data/.bss` but no `.got`/`.got.plt`/
  `.data.rel.ro`/`.init_array`; GNU ld and lld place orphans differently, so the
  "same ELF everywhere" promise holds only while zero orphans appear. Add an explicit
  orphan sink (and/or `ASSERT` that they're empty), and add a one-line comment that the
  single PT_LOAD being **RWE** is a deliberate test-guest choice. The e_entry-coverage
  test is the current backstop — note that. (I-4)

- [ ] **Document and (optionally) strengthen the linker probe.**
  `tests/nanokernel/build.rs` `probe()` relies on GNU ld validating `-m` before `--version`
  (verified: exit 1 on unsupported emulation). Add a comment recording this fact so it
  isn't refactored away, and consider replacing the `--version` probe with a real
  empty-object link against `link.ld` to catch flag/script skew at probe time. (I-1)

---

### Suggestions

- [ ] **Declare `rerun-if-env-changed` for `RUSTC` and `HOST`** in `build.rs` (linker
  selection branches on both; cargo sets them but won't rerun the script if they change). (S-1)
- [ ] **Match `%define` tokens exactly** in `tests/elf_shape.rs` (`split_whitespace` token
  compare) instead of the trailing-space `contains()` convention, so reformatting the
  `.inc` can't break the drift guard. (S-2)
- [ ] **Pin `reserved` (0x1C) and total header size (0x20)** in both `bootinfo.inc` and
  `src/lib.rs` so the full header layout — not just consumed fields — is drift-checked. (S-3)
- [ ] **Cross-reference the loader bead/file that owns the fixed BootInfo GPA** in the
  `bootinfo.inc` header comment, so both halves of the contract are navigable. (S-4)
- [ ] **Document crt0's stack-alignment guarantee** (`stack_top` is 16-aligned ⇒ post-CALL
  `RSP%16==8` in `prog_main`); the `align 16` before `stack_bottom` is load-bearing. (S-5)
- [ ] **Assert `p_memsz > p_filesz`** for the entry-covering PT_LOAD in `elf_shape.rs`
  (read `p_filesz` at `at + 32`) to make the loader's bss zero-fill obligation an
  executable guest-side invariant. (S-6)
