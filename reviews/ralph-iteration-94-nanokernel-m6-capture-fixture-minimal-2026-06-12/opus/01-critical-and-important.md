# Critical and Important Findings

## Critical

**None.** I checked the points most likely to harbor real bugs and all are correct:

- **NASM imm32 sign-extension on the qword stores.** `mov qword [rdi + OFF_ENTRY0 + 24],
  FB_BYTES` (0x10000), `mov qword [rdi], FB_GPA` (0x600000), and `mov qword [rdi + 8],
  FB_BYTES` all use values that fit in a *positive* imm32, so the sign-extension to 64 bits
  preserves the intended value. No truncation or sign-flip. `mov rax, FB_QWORD_BASE`
  (0xFB00…) is `mov r64, imm64` and is fine.
- **The `0x00400001` dword pack at manifest+4.** Little-endian bytes are `01 00 40 00`;
  bytes [4..6] = manifest_version u16 = 1, bytes [6..8] = region_capacity u16 = 64. Matches
  `detguest-wire` (`MANIFEST_VERSION = 1`, `REGION_CAPACITY = 64`) and the interop test's
  two separate u16 writes produce the identical bytes.
- **`rep movsb` register state.** rdi/rsi/rcx are all loaded immediately before the copy
  (`lea rsi`, `lea rdi`, `mov rcx, REGION_NAME_LEN`); DF=0 is guaranteed because crt0
  (`asm/crt0.asm`) executes `cld` before `call prog_main`. Forward copy of 11 bytes is
  correct.
- **`loop .fill` rcx usage.** rcx is set to `FB_QWORDS` (8192) and is not touched inside the
  body, so the 8192-iteration framebuffer fill is correct.
- **Manifest field offsets vs `detguest-wire`.** entry stride 96, entry0 at area+0x20,
  extent0 at area+0x1820, generation at +8 — all confirmed against
  `detguest-wire/src/manifest.rs`. The asm stores name_id at +4, layout_version at +8, flags
  at +12, len at +24, extent_n at +36, name at +40; every one matches `RegionEntry::read_from`.
- **Generation stays even (0).** Guest RAM is zeroed and the asm never writes the generation
  word; the interop `MockGuestMem::with_zeroed` leaves it 0 too. The host's first reader runs
  at attach after all stores, so no seqlock dance is needed — the module-header reasoning is
  sound.
- **Framebuffer / channel non-overlap.** FB_GPA 0x60_0000 == CHANNEL_GPA 0x40_0000 + channel
  span 0x20_0000 (512 pages). No overlap; the `const _FB_CLEAR_OF_CHANNEL` assert pins it.
- **Wire bytes correct end-to-end.** The interop test runs the real `Channel::attach` /
  `read_manifest` / `resolve` / `read_region` and passes — proving the asm-equivalent bytes
  decode correctly, that `read_region` walks the single extent into the known pattern, and
  that an over-read past `region.len` is refused. The W ring size 0x100000 is a power of two,
  so the rebuilt header passes attach's `is_power_of_two` check.

## Important

### I1 — Channel header ring-desc literals are not drift-pinned to `DEVICE_EXERCISE_RING_DESCS`

- **File:** `tests/nanokernel/asm/capture_fixture.asm:121-141` (the `mov dword [rbx + 0x10]`
  … `[rbx + 0x2C]` block) vs `tests/nanokernel/tests/capture_manifest_interop.rs:30-40`
  (which builds the header *from* `DEVICE_EXERCISE_RING_DESCS`).
- **Severity:** Important (drift-pin gap; no current incorrectness).
- **Description:** The asm hardcodes the channel header's ring descriptors as literal dwords
  (`0x8000/0x4000/0xC000/0x4000/0x10000/0x10000/0x20000/0x100000`). The interop test does not
  read those asm bytes — it *rebuilds* the header from the Rust constant
  `DEVICE_EXERCISE_RING_DESCS`. Consequently, if the asm header literals were ever edited to
  diverge from the constant, the interop test would still pass (it never sees the asm's header
  bytes), and `attach` would be exercised against the Rust-built header, not the asm's. The
  drift pin `capture_fixture_asm_matches_rust_constants` covers the manifest constants
  (MANIFEST_MAGIC, MANIFEST_OFF, OFF_ENTRY0, OFF_EXTENT0, REGION_FLAG_FRAMEBUFFER, GPAs,
  pattern base) but *not* the eight header ring-desc literals. Note: the manifest *entry*
  field offsets (+4/+8/+12/+24/+36/+40) are safe because the interop test runs the real
  `RegionEntry::read_from` codec, which would reject a layout mismatch — but the header
  literals get no such cross-check.
- **Caveat:** This is a pre-existing pattern. `device_exercise.asm` hardcodes the same header
  literals and its `channel_interop.rs` similarly rebuilds from the constant, so this fixture
  is consistent with the established convention rather than introducing a regression.
- **Suggested fix:** Add a header drift assertion to
  `capture_fixture_asm_matches_rust_constants` that scrapes the eight `mov dword [rbx + …]`
  header stores from the asm and checks them against `DEVICE_EXERCISE_RING_DESCS` (and the
  `0x08`/`0x0C` proto/flags). Even a lightweight substring check (`asm.contains("0x8000")`
  etc.) would catch an accidental header edit:

  ```rust
  // The asm's hardcoded ring-desc literals must equal the constant the
  // interop test rebuilds the header from — otherwise the asm header
  // could drift without any test noticing.
  for (off, size) in DEVICE_EXERCISE_RING_DESCS {
      assert!(asm.contains(&format!("{off:#x}")), "header ring offset {off:#x} missing from asm");
      assert!(asm.contains(&format!("{size:#x}")), "header ring size {size:#x} missing from asm");
  }
  ```

  (A stricter version would parse the specific `mov dword [rbx + 0x10]` … rows; the substring
  form is the minimal guard.)
