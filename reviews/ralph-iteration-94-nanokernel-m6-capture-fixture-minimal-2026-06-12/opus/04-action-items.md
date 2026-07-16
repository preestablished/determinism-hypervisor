# Action Items

### Critical
- [ ] None.

### Important
- [ ] [tests/nanokernel/tests/elf_shape.rs:330-396 + asm/capture_fixture.asm:121-141] Add a
  drift assertion that pins the asm's eight hardcoded channel-header ring-desc literals
  (`0x8000/0x4000/0xC000/0x4000/0x10000/0x10000/0x20000/0x100000`) to
  `DEVICE_EXERCISE_RING_DESCS`. The interop test rebuilds the header *from* that constant and
  never reads the asm header bytes, so an accidental edit to the asm header would go
  uncaught. A substring-contains loop over the constant is the minimal guard; a per-row parse
  is stricter. (Pre-existing pattern shared with `device_exercise`; non-fatal, but this is the
  one real coverage gap.)

### Suggestions
- [ ] [asm/capture_fixture.asm:75-99] Mirror `landing_loop`'s `BOOTINFO_MAGIC` check before
  parsing cmdline digits, to honor the stated "same parse contract as landing_loop" and guard
  against a malformed BootInfo. Fold the `cmp dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC`
  into the existing null/mem_size checks.
- [ ] [asm/capture_fixture.asm:47] Add a one-line comment on `%define MANIFEST_OFF 0x1000`
  cross-referencing `detguest-wire::header::OFF_MANIFEST` (drift is already pinned; this is
  readability/provenance parity with the `OFF_ENTRY0`/`OFF_EXTENT0` annotations).
- [ ] [tests/nanokernel/tests/capture_manifest_interop.rs:18-26] Optional comment in
  `guest_built_memory` explaining that the single mock spans channel-page + gap + framebuffer
  and that the const assert in `elf_shape.rs` makes that safe.
- [ ] [tests/nanokernel/tests/capture_manifest_interop.rs:84] Optionally add
  `resolve("framebuffe")` / `resolve("framebufferX")` → `None` assertions to lock the
  NUL-terminated name comparison as exact (not a prefix match).
