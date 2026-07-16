# Suggestions (non-blocking)

## S1 — run.rs hardcodes the PIO range instead of reusing the constants

`tools/dh-cli/src/run.rs:67` and `:71` use the literal `(0x3F8..0x400)` while `boot.rs` was
deliberately refactored (`boot.rs:18-19`) to derive `SERIAL_BASE`/`SERIAL_END` from
`SERIAL_PIO_BASE`/`SERIAL_PIO_LEN`. Reuse the same constants in `run.rs` so the two debug loops
cannot drift if the range ever changes.

```rust
use dh_devices::serial::{SERIAL_PIO_BASE, SERIAL_PIO_LEN};
const SERIAL_BASE: u16 = SERIAL_PIO_BASE;
const SERIAL_END: u16 = SERIAL_PIO_BASE + SERIAL_PIO_LEN;
// ... VcpuExit::IoOut(port, data) if (SERIAL_BASE..SERIAL_END).contains(&port) => ...
```

## S2 — run.rs serial IN path has no automated test coverage

The IoIn arm added at `run.rs:71` is exercised only by the (untested) `dh-cli run` binary path;
`phase1_gate_smoke` runs the gate, not a serial-IN guest. The path is structurally identical to
the live-proven `boot.rs` IN arm and the prior OUT arm, so risk is low — but a small test that
drives `run::run` with `hello.elf` (which now polls LSR via IN) and asserts the serial output
would close the gap and lock in the run-loop IN behavior under the single-step/PMI landing.

## S3 — multi-byte IN fidelity is documented only in code comments

`pio_read`'s "every byte of a wider access reads the same register" behavior (`serial.rs:69-71`)
is correct for determinism but surprising for anyone expecting per-port word reads. The comment
is good; consider a one-line note that this is a deliberate determinism-over-fidelity choice
(real 16550 word IN would read consecutive registers), so a future reader doesn't "fix" it into
host-order-dependent behavior.

## S4 — 8-byte MMIO read of a register slot silently returns 0

The bus permits 8-byte aligned accesses (`bus.rs:74-83`); a guest doing an 8-byte read at window
offset 0x08 reaches `mmio_read`, whose guard `data.len() != 4` makes it `data.fill(0)`
(`serial.rs:62-63`). This is deterministic and conforms to the trait's "unknown reads as zeros"
rule, but a 16550 register read at 8-byte width returning 0 (rather than the 4-byte register
value zero-extended) is a slightly odd corner. No guest is expected to do this; leaving it as
RAZ is fine — flag only so it's a conscious choice.
