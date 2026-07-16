# Positive notes

- **The hard part is the doc-vs-code judgment, and it was made correctly.** The
  bead/plan say "real-mode→long-mode stub"; the author recognized ARCH §2.3's
  direct-long-mode entry makes that phase the VMM's job, wrote a pure `BITS 64`
  program, and *documented the divergence in the file header* (lines 11–13). That
  header turns a future "wait, where's the mode switch?" into a non-event. This is
  exactly the right way to handle a stale-spec/normative-spec conflict.

- **Minimal and idiomatic.** `lea` / `mov rcx` / `lodsb` / `out` / `loop` / `ret`
  is the tightest correct way to spray N bytes to a PIO port. It leans on crt0's
  `cld` and HLT park instead of re-implementing them, matching the established
  `pipeline_smoke.asm` convention (`global prog_main`, single-byte `out dx, al`,
  `ret` into the park).

- **Wiring is complete and consistent.** The new program is added in all four
  places it needs to be — `build.rs` `PROGRAMS`, `hello_elf()` accessor, the lib
  smoke test, and the `elf_shape` sweep — so it is built, shape-checked
  (static x86-64 exec, `e_entry == 0x100000`, PT_LOAD covers entry, size budget)
  and non-empty-asserted from the first commit. No half-wired artifact.

- **`HELLO_SERIAL_OUTPUT` is a forward-looking, well-placed contract.** Exporting
  the expected bytes as a `pub const` next to the ELF accessor gives bead 1mz
  (dh-cli boot) a single source of truth to grep against, instead of hard-coding
  `b"HELLO\n"` at the call site. It is currently unused but that is a deliberate
  forward reference, called out in the doc comment.

- **Comments cite their sources.** Every non-obvious decision points at a doc
  section or bead (ARCH §2.3, bead 1mz, the HLT-park contract), keeping the stub
  self-explaining.

- **Builds clean, tests green, disassembly matches intent** — verified end to end,
  not just "it compiles."
