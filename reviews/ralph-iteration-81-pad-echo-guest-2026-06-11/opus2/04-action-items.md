# Action Items

### Critical

_None._

### Important

- [ ] **Bound the table or document its hard capacity before the M5 run is
  scheduled against this guest.** The frame loop appends 8 bytes/frame with no
  cap; at the test-default 64 MiB guest the table runs off the end of mapped RAM
  after ~8.0M frames (~3.2 s-vns at 1:1, ~400 instrs/frame), versus the ~150M
  frames a 60 s-vns run would need (~1.2 GB, ~19× the whole guest). A store past
  `mem_bytes` faults as an unmapped EPT violation / unexpected exit mid-table,
  killing the acceptance run. Fix: add a `%define COUNT_MASK` and
  `and rcx, COUNT_MASK` to make the table a fixed-size ring (drift-pinned in
  `lib.rs`), OR state a concrete max-frames-vs-min-`mem_bytes` budget in both the
  asm header and `lib.rs` and require the M5 run to bound its icount below it.
  At minimum, replace the header's silence on where growth stops with an explicit
  capacity statement. (`tests/nanokernel/asm/pad_echo.asm:40-46`; boundary
  behavior per `crates/dh-vmm/src/boot.rs:145` + `runctl.rs:716`.)

- [ ] **Widen the drift pin to cover every device-coupled constant and the entry
  stride.** Today only `TABLE_GPA`, `PACE_ITERS`, `PAD_BASE` are pinned. Add
  pins for `REG_PAD0` (vs `dh_devices::pad::REG_PAD0` = `0x08`), `REG_FRAME` (vs
  `pad::REG_FRAME_COUNTER` = `0x1C`), and `SERIAL_PORT` (vs
  `serial::SERIAL_PIO_BASE` = `0x3F8`). Also introduce `%define ENTRY_BYTES 8`
  (and `HEADER_BYTES 8`) in the asm, use them in `lea rdx, [r9 + 8 + rcx*8]`, and
  pin them against `PAD_ECHO_ENTRY_BYTES` — the literal `8`/`*8` in the
  addressing mode is currently invisible to the drift test, so the host-side
  layout mirror can drift silently. (`tests/nanokernel/tests/elf_shape.rs:248-250`,
  `tests/nanokernel/src/lib.rs:90-96`.)

### Suggestions

- [ ] Make the `PAD_BASE`/register pins import `dh_devices` constants directly
  instead of hardcoded literals + comments. `nanokernel` can add `dh-devices` as
  a `[dev-dependency]` with no dependency cycle (`dh-devices` does not depend on
  `nanokernel`/`dh-vmm`, and `nanokernel` already dev-deps the same
  `detguest-host`/`detguest-wire` crates). This is the stronger pin; weigh the
  new dev-only cross-crate build edge. (`tests/nanokernel/Cargo.toml`,
  `tests/nanokernel/tests/elf_shape.rs:250`.)

- [ ] Delete the unused `extern BOOT_INFO_PTR` (`pad_echo.asm:28`) — the guest
  never references it, unlike every other guest that declares it (all use it
  twice); `hello.asm` correctly omits it.

- [ ] Reword the header's "RAM is zeroed at boot so count starts at 0"
  (`pad_echo.asm:14-15`): the table's zero comes from the fresh anonymous mmap,
  not the loader's bss zero-fill (which only covers `work_buf`). Correct on every
  path, but the comment credits the wrong mechanism.

- [ ] Add a one-line note on `work_buf`'s purpose (`pad_echo.asm:67-69`): the
  pace-loop store target exists only to fix the per-iteration instruction count;
  removing/optimizing it would silently change the per-frame icount and desync
  every drift-pinned frame boundary. (The `and ebx, 511` mask already bounds it
  safely to the 512-qword buffer.)
