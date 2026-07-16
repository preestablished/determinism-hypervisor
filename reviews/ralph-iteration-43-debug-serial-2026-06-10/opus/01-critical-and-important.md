# Critical and Important Issues

**None found.**

I went after this adversarially — the brief flagged seven specific things to interrogate. Each
resolved cleanly under inspection and live execution:

1. **§6.9 + §2.2 conformance — clean.** `DEVICE_ID_DEBUG_SERIAL = 0x0006` is unique and
   sequential after blk (0x0005); window 0xD000_6000 matches the §2.2 layout
   (`ARCHITECTURE.md:134`); PIO base 0x3F8 / len 8 matches §6.9 and the old hardcoded
   `0x3F8..0x400`. Register slots at `0x08 + reg*4` match the established §6.1 device convention
   (`bus.rs:14` `REG_DEVICE_BASE = 0x08`); MAGIC/VERSION (0x00/0x04) are bus-owned, so the
   device's `mmio_read` is never invoked for them and its `0x08..0x28` guard cannot conflict with
   the device-id read (`serial.rs:174` exercises the bus REG_MAGIC arm, not `mmio_read`).

2. **Empty snapshot — correct against the framing.** `device_sections` (`hash.rs:333`) frames
   every device as `(device_id:u16, section_version:u16, len:u32, bytes)`. A zero-length section
   yields a fully unambiguous frame (id, version, len=0, no bytes) — it does not desync the
   framing or the restore path. The model is also **not** registered on the hashed bus in either
   run loop (it is a standalone struct in `boot.rs`/`run.rs`), so serial state never enters the
   hash at all, exactly per §6.9. `restore` is strict (rejects non-empty bytes and wrong
   version) and matches the `pad.rs`/`clock.rs`/`entropy.rs` convention.

3. **Multi-byte IN fill — deterministic, acceptable.** `pio_read` does
   `data.fill(reg_read(first_reg))` (`serial.rs:71`), so a word IN spanning two registers reads
   the first register's value into both bytes rather than each port's own value. This diverges
   from real-hardware fidelity (e.g. a word read at LSR+MSR would return 0x60,0x60 instead of
   0x60,0x00) but is a fixed, host-independent mapping — determinism, not fidelity, is the §6.9
   bar, and the model is output-only so guests don't rely on RX content. Not a bug; noted in
   suggestions only.

4. **boot.rs IN-FILL contract — preserved.** The serial-range arms (`boot.rs:78`, `:82`) match
   *before* the `classify_exit` fallthrough, so the fill happens on the raw `VcpuExit` data slice
   (which kvm-ioctls writes back on the next KVM_RUN) — `classify_exit` never sees the serial
   ports in this loop. No port off-by-one: `SERIAL_BASE..SERIAL_END` = `0x3F8..0x400` covers all
   8 registers, identical to the prior constant.

5. **run.rs on_exit — fill reaches the guest.** `on_exit` receives the raw `VcpuExit` with the
   live `&mut [u8]` (`boundary.rs:143/166/210`); filling it under either the PMI far-approach or
   the single-step near-approach is structurally identical to the already-proven OUT path, and
   the IN completes on the subsequent `guard.run()`. The serial IN at a single-step boundary does
   not misbehave: KVM_EXIT_IO fires mid-instruction (before retirement), the counter only moves
   on retirement, so no double-count.

6. **hello.asm — correct asm, live-proven.** `SERIAL_LSR=0x3FD` is reg 5; `LSR_THRE=0x20` is
   bit 5 (THR empty); DebugSerial's LSR reads 0x60 (bits 5+6), so `test al,0x20; jz` falls
   through. `dx` is reloaded to `SERIAL_PORT` before the `out`. `cargo test -p dh-cli --test
   boot_hello` passes with `out.serial == b"HELLO\n"`.

7. **Determinism — no host-state dependency.** `out` is a pure accumulator; `reg_read` is a
   const mapping; `take_output` uses `std::mem::take`; `restore` clears `out`. No time, no
   randomness, no host IO. The dh-devices `no_host_ambient_authority` grep gate covers the file.
   The `landing_loop_is_deterministic_across_runs` test (run-twice equality, serial included)
   passes.
