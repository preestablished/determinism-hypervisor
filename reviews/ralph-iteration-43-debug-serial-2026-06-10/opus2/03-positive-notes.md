# Positive Notes (patterns worth preserving)

### P1 — The LSR-always-ready design directly kills the iter-29 hazard, and `hello.asm` proves it live

`LSR_ALWAYS_READY = 0x60` (THR-empty bit 5 + transmitter-empty bit 6) means an LSR-polling
16550 driver reads "ready" on its first poll and never spins. `hello.asm` was rewritten to
poll LSR before each byte (`mov dx, 0x3FD; in al, dx; test al, 0x20; jz .wait_thre`) — so
the guest now exercises the exact driver discipline that hung forever under the old blanket
zero-fill. My exit-count experiment confirmed hello takes 13 exits (6 IN + 6 OUT + HLT)
with the poll succeeding on the first IN every byte. This is the right kind of regression
guard: the test guest *is* the hazard, and it passes only because the device answers
correctly.

### P2 — Snapshot section is empty *by design* and round-trips cleanly through the hash framing

`snapshot()` is intentionally a no-op and `restore()` rejects any non-empty payload — the
correct encoding of "serial output is host observability, never machine state, never in the
state hash." I verified via a live experiment that the empty section round-trips through
`dh_vmm::hash::device_sections` as `id=0x0006 ver=1 len=0` (8 bytes) and stays
self-delimiting next to a second device. The `restore` also `clear()`s the pending buffer,
correctly treating a restored slot as starting with no host-side output. The comments
explain *why* it's empty, which is exactly the kind of intent a future snapshot author
needs.

### P3 — Both debug loops fill serial INs on the RAW exit — the only correct seam

`boot.rs` and `run.rs` match `VcpuExit::IoIn(port, data)` for the serial range *before*
any classify step, so they hold the live `&mut [u8]` and fill it via `pio_read` before
re-entry. This sidesteps the stale-buffer nondeterminism that classify_exit's value-only
`SerialIn { port, len }` variant would invite. (The matching documentation gap is I1 — the
code itself does the right thing.)

### P4 — Match-arm ordering and range bounds are exactly right

In `boot.rs`, the serial `IoIn` arm precedes the generic `data.fill(0)` zero-fill arm, so
serial ports are served and all other ports still RAZ — the iter-29 blanket fill is
replaced surgically, not removed. `SERIAL_END = SERIAL_PIO_BASE + SERIAL_PIO_LEN = 0x400`
exactly, so `(0x3F8..0x400)` covers all 8 registers and excludes 0x400. `BootOutcome.serial`
stayed `Vec<u8>` (filled via `take_output()`), so every existing consumer
(`main.rs`, `boot_hello.rs`) is unaffected.

### P5 — Unit tests assert behavior, not implementation

`output_accumulates_and_drains` checks that an IER write (0x3F9) is swallowed while THR
output accumulates and drains. `lsr_polls_ready_and_rx_reads_zero` pins LSR=0x60, RBR=0,
IIR=0x01 with human-readable rationale strings. `snapshot_is_empty_and_restore_clears_pending`
asserts the empty-section invariant AND both restore rejection paths (non-empty bytes, wrong
version). These tests would catch a real regression, not just lock in the current code.

### P6 — MMIO mirror geometry is documented and bounded

The `0x08 + reg*4` slot mapping, the `(0x08..0x28)` range, and the 4-byte alignment check
are all spelled out in the doc comment with the rationale ("byte-packing them would be
unreachable" because the bus enforces 4-byte alignment). The unknown-offset and
wrong-length paths fall to `data.fill(0)` — honoring the trait contract that unknown
offsets read zeros, never host state.
