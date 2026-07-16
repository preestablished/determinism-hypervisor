# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [tools/dh-cli/src/run.rs:67,71] Replace the hardcoded `(0x3F8..0x400)` serial range with
  `SERIAL_PIO_BASE`/`SERIAL_PIO_LEN`-derived constants (as `boot.rs:18-19` already does) so the
  two debug loops cannot drift if the PIO range changes.
- [ ] [tools/dh-cli/src/run.rs:71] Add a test that drives `run::run` with `hello.elf` (which now
  polls LSR via IN) and asserts `serial == b"HELLO\n"`, to give the run-loop serial-IN arm direct
  coverage under the single-step/PMI landing (currently only `boot.rs`'s IN arm is tested).
- [ ] [crates/dh-devices/src/serial.rs:69-71] Add a one-line comment that the
  `data.fill(reg_read(first_reg))` multi-byte behavior is a deliberate determinism-over-fidelity
  choice (real 16550 word IN reads consecutive registers), to prevent a future "fix" into
  host-order-dependent reads.
- [ ] [crates/dh-devices/src/serial.rs:62-63] Confirm intent that an 8-byte aligned MMIO read of
  a register slot returns 0 (current RAZ behavior) rather than the zero-extended 4-byte value; fine
  as-is, just make it a conscious documented choice.
