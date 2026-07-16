# Action Items

### Critical

_None._

### Important

- [ ] [crates/dh-vmm/src/kvm.rs:265] Add a doc comment on `ExitEvent::SerialIn { port, len }`
  warning that, like `DetcallIn`, the serial RX buffer MUST be filled on the **raw**
  `VcpuExit::IoIn` exit before re-entry (via `DebugSerial::pio_read`) — this event carries
  only `len`, so a `classify_exit`-based run loop CANNOT fill it and would resume the guest
  with stale kvm_run bytes, re-arming the iter-29 nondeterminism hazard. Optionally add a
  matching one-line warning at the future M1 run-loop entry point. (Documentation only; the
  two debug loops shipped here already do the right thing.)

### Suggestions

- [ ] [tools/dh-cli/src/run.rs:67,71] Replace the hardcoded `0x3F8..0x400` literals with
  `dh_devices::serial::{SERIAL_PIO_BASE, SERIAL_PIO_LEN}` (mirroring `boot.rs`), since
  `run.rs` already constructs a `DebugSerial`. The range now has 4 edit sites; consolidate
  the dh-cli ones. (S1)
- [ ] [crates/dh-devices/src/serial.rs:159] Add a `pio_read` test with a 2-byte buffer
  asserting `[0x60, 0x60]` for LSR (documents wide-access-reads-same-register), plus
  out-of-range cases `pio_read(0x3F7, ..)` and `pio_read(0x401, ..)` returning 0 — pin the
  `saturating_sub` behavior. (S2)
- [ ] [crates/dh-devices/src/serial.rs:62] Add a `debug_assert!` or doc precondition on
  `pio_write`/`pio_read` stating the caller must pass a port within
  `SERIAL_PIO_BASE..SERIAL_PIO_BASE+SERIAL_PIO_LEN`; the pub API currently silently maps
  out-of-range ports to reg 0 / swallows. (S3)
- [ ] [crates/dh-devices/src/serial.rs:73] Document (and ideally test) that `pio_write`
  intentionally logs **every byte** of a multi-byte OUT, whereas the MMIO THR mirror logs
  only `data[0]` — make the transport asymmetry an explicit, JSON-log-consumer-aware
  decision. (S4)
- [ ] [crates/dh-devices/src/serial.rs:99] Add a one-line comment to `mmio_write`/`mmio_read`
  noting that 8-byte accesses (legal per the bus) are not part of the 4-byte §6.9 mirror and
  are deliberately swallowed/zeroed. (S5)
- [ ] [crates/dh-devices/src/serial.rs:50] Note that `reg_read` is an associated fn (no
  `&self`) deliberately — register reads must never depend on state, which is the invariant
  that keeps the device output-only. (S6)
