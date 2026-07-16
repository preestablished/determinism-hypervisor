# Suggestions (non-blocking)

### S1 — cmdline parse omits the BootInfo magic check that `landing_loop` performs

- **File:** `tests/nanokernel/asm/capture_fixture.asm:75-99` (the `.parse` setup) vs
  `tests/nanokernel/asm/landing_loop.asm:36-40`.
- **What:** `landing_loop` validates `cmp dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC` /
  `jne .have_count` before touching cmdline fields. `capture_fixture` checks only that the
  BootInfo pointer is non-null, then reads `cmdline_len` and `cmdline` directly.
- **Why:** In practice dh-vmm always supplies a valid BootInfo, so this is not a live bug.
  But the module header explicitly claims "same parse contract as landing_loop," and the
  magic check is part of that contract. Adding it makes the two guests genuinely identical
  and guards against a malformed/zeroed BootInfo causing the parse loop to walk garbage
  `cmdline_len` bytes.
- **Snippet:**
  ```nasm
      mov     rsi, [BOOT_INFO_PTR]
      test    rsi, rsi
      jz      .fail_f
      cmp     dword [rsi + BOOTINFO_OFF_MAGIC], BOOTINFO_MAGIC
      jne     .have_version            ; no valid cmdline → default version
      mov     rax, [rsi + BOOTINFO_OFF_MEM_SIZE]
      ...
  ```
  (Placement differs slightly because `capture_fixture` already dereferences mem_size; fold
  the magic check in alongside the existing null check.)

### S2 — `MANIFEST_OFF` is a private `%define`, not derived from `detguest-wire`

- **File:** `tests/nanokernel/asm/capture_fixture.asm:47` (`%define MANIFEST_OFF 0x1000`).
- **What:** The asm hardcodes `0x1000`. The drift test does pin it
  (`elf_shape.rs` asserts `define("MANIFEST_OFF") == OFF_MANIFEST`), so drift is caught — this
  is purely a readability note.
- **Why:** A one-line comment cross-referencing `detguest-wire::header::OFF_MANIFEST` (the
  way the module header already cites "API.md §4.1") would make the provenance explicit for
  the next editor, matching how `OFF_ENTRY0` / `OFF_EXTENT0` are annotated.

### S3 — Interop test could assert the framebuffer span fits the channel-page gap

- **File:** `tests/nanokernel/tests/capture_manifest_interop.rs:18-26` (`guest_built_memory`).
- **What:** The helper allocates `span = (FB_GPA + FB_BYTES) - CHANNEL_GPA` and writes both
  the channel page and the framebuffer into one mock. The drift test already asserts
  `FB_GPA >= CHANNEL_GPA + 0x20_0000` at compile time, so the layout is sound.
- **Why:** A short comment in the helper noting that the single mock spans channel-page +
  gap + framebuffer (and why that's safe — the const assert) would save the next reader from
  re-deriving the arithmetic. Optional; the existing comment is already decent.

### S4 — Consider exercising the `name_bytes()` NUL-padding path explicitly

- **File:** `tests/nanokernel/tests/capture_manifest_interop.rs:84` (`resolve("framebuffer")`).
- **What:** The name `"framebuffer"` is 11 bytes written into a 56-byte NUL-padded field; the
  asm relies on zeroed RAM for the padding. `resolve` already matches via `name_bytes()`
  (truncating at the first NUL), so the happy path is covered.
- **Why:** A tiny assertion that `resolve("framebuffe")` (a prefix) and `resolve("framebufferX")`
  both return `None` would lock in that the NUL-terminated name comparison is exact, not a
  prefix match — cheap insurance for the wire-name contract. Non-blocking.
