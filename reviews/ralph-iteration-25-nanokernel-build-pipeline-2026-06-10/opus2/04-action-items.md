# Action Items

Each item is self-contained: file, problem, and concrete fix.

### Critical

_None._

---

### Important

- **[I-1] Make the drift-test parser exact-match, not substring-match.**
  `tests/nanokernel/tests/elf_shape.rs:60-92`. The `lookup` closure matches on
  `l.contains(name)` and returns the first hit; it is correct *only* because every
  caller passes a trailing space that happens to disambiguate `BOOTINFO_OFF_CMDLINE`
  from the earlier `BOOTINFO_OFF_CMDLINE_LEN`. A future caller without the trailing
  space would silently match the wrong `%define`. Fix: match the symbol as a whole
  token —
  `.find(|l| l.starts_with("%define") && l.split_whitespace().nth(1) == Some(name))` —
  and drop the trailing spaces from the call sites. Closes the silent-pass hole in
  the very test meant to prevent ABI drift.

- **[I-2] Establish a single source of truth for the BootInfo binary layout.**
  `include/bootinfo.inc:1`, `src/lib.rs:1-9`. Both cite "ARCH §2.3" for magic /
  version / offsets / `reserved`, but §2.3 only describes "a versioned struct …
  carrying mem_size, MMIO base, cmdline bytes" — none of the binary layout. The
  loader bead will read §2.3 and not find the contract these files mirror. Fix: add
  the full BootInfo layout table (magic value, version offset, all `OFF_*`, reserved)
  to ARCHITECTURE.md §2.3 as normative and have the code say "mirrors §2.3 Table N,"
  OR change the comments to declare `include/bootinfo.inc` the normative layout for
  now and file a bead to fold it into §2.3 before the loader is written.

- **[I-3] Document the PT_LOAD `memsz > filesz` zero-fill requirement for the loader.**
  `asm/crt0.asm:8,27-33`, `link.ld:19`, ARCH §2.3. The emitted ELF has
  `p_filesz=0x47` / `p_memsz=0x4060`; the 16409-byte `.bss` tail (stack +
  `BOOT_INFO_PTR`) exists only as memsz. crt0 promises guests "a zeroed `.bss`," but
  no doc tells the §2.3 loader that it must zero-fill `[p_filesz, p_memsz)`. Fix: add
  one sentence to §2.3 / the loader-bead contract: "PT_LOAD segments with
  `p_memsz > p_filesz` must be zero-filled over `[p_filesz, p_memsz)` in guest RAM."
  Optionally assert `memsz > filesz` in `elf_shape.rs` so the loader has a fixture
  that exercises its zero-fill path.

---

### Suggestions

- **[S-1] Guard nasm on the `kvm-intel` CI lane.** `.github/workflows/ci.yaml:80-104`.
  That lane builds the workspace (now including `tests/nanokernel`) with no nasm
  guard; a re-imaged box yields an opaque `build.rs` panic. Add the host lane's
  guard, or a `command -v nasm || { echo "::error::nasm missing"; exit 1; }` precheck.

- **[S-2] Note that nanokernel images use a single RWE PT_LOAD (no W^X).**
  `link.ld`. Section flags are clean but the load segment is RWE; a W^X-enforcing
  loader would map `.text` writable. Document it, or split RX/RW segments via `PHDRS`.

- **[S-3] Tighten the ELF size budget.** `tests/nanokernel/tests/elf_shape.rs:50-54`.
  Real image is 5144 B file / 71 B `p_filesz`; the 64 KiB ceiling barely constrains
  anything. Assert `< 16*1024` on file size, or assert on `p_filesz` to actually
  catch accidental fat-object links.

- **[S-4] Mirror the `reserved` field (0x1C) in the offset table.**
  `include/bootinfo.inc`, `src/lib.rs`. Add `BOOTINFO_OFF_RESERVED = 0x1C` on both
  sides and a drift assertion so the last word in the table is covered.

- **[S-5] Bounds-check `elf_shape.rs` ELF reads.** `tests/nanokernel/tests/elf_shape.rs`.
  Add `assert!(elf.len() >= 64)` and `phoff + phnum*phentsize <= elf.len()` so a
  degenerate image fails with a readable message instead of an index panic.

- **[S-6] Update the stale README.** `tests/nanokernel/README.md` claims the pipeline
  is "implemented by later beads" — but this iteration implements it. Rewrite to
  describe the now-present `build.rs`/nasm/link pipeline and the `pipeline_smoke`
  fixture.
