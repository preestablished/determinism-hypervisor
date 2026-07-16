# Suggestions (non-blocking)

### S1 — Triple-defined serial PIO range; `run.rs`/`runctl.rs` still hardcode the literal

**Files:** `tools/dh-cli/src/run.rs:67,71`; `crates/dh-vmm/src/runctl.rs:711`;
`crates/dh-vmm/src/kvm.rs:24-25` (`PIO_SERIAL_BASE`/`PIO_SERIAL_LEN`);
`crates/dh-devices/src/serial.rs:17-18` (`SERIAL_PIO_BASE`/`SERIAL_PIO_LEN`).

The serial range now has **three** spellings: the new `SERIAL_PIO_BASE`/`SERIAL_PIO_LEN`
in dh-devices, the pre-existing `PIO_SERIAL_BASE`/`PIO_SERIAL_LEN` in dh-vmm::kvm, and the
hardcoded `0x3F8..0x400` literals in `run.rs` (twice) and `runctl.rs`. `boot.rs`
deduplicated against the dh-devices constants — good — but `run.rs`, which *also*
constructs a `dh_devices::DebugSerial`, still hardcodes the literal, and so does the
`halt_tests` harness in `runctl.rs`. A future range change (or an off-by-one) now has four
edit sites to keep in sync. Have `run.rs` import and use `dh_devices::serial::{SERIAL_PIO_BASE,
SERIAL_PIO_LEN}` like `boot.rs` does. (Whether dh-vmm's own `PIO_SERIAL_*` should also be
unified is a layering judgment — dh-vmm intentionally doesn't depend on dh-devices — so
leaving those two is defensible, but the bare literals in dh-cli should not exist.)

### S2 — No test for multi-byte IN or out-of-range `pio_read`

**File:** `crates/dh-devices/src/serial.rs` tests (lines 134-198).

`pio_write` multi-byte is covered (`pio_write(0x3F8, b"HE")`), but every `pio_read` test
uses a 1-byte buffer. Add a 2-byte IN test asserting `pio_read(0x3FD, &mut [0;2])` yields
`[0x60, 0x60]` (documents the "every byte of a wider access reads the same register"
contract — relevant because `in ax, dx` on real hardware reads LSR+MSR, here both are LSR).
Also add an out-of-range case: `pio_read(0x3F7, ...)` (below base → saturates to reg 0 → 0)
and `pio_read(0x401, ...)` (above range → reg 9 → `_ => 0`), pinning the saturating-sub
behavior so a future refactor can't silently change it.

### S3 — `pub` PIO API has no range guard; rely on caller discipline only

**File:** `crates/dh-devices/src/serial.rs:62-77` (`pio_write`, `pio_read`).

Both methods are `pub` and accept any `u16` port. `pio_read` uses
`port.saturating_sub(SERIAL_PIO_BASE)`, so a port below 0x3F8 silently maps to reg 0
(returns 0) and a port above 0x3FF maps to reg ≥ 8 (returns 0). `pio_write` only transmits
on exactly `0x3F8`, swallowing everything else. All current callers guard
`(0x3F8..0x400).contains(&port)`, so this is benign today, but the pub contract is
permissive in a way that hides a miswired caller. Consider a `debug_assert!((SERIAL_PIO_BASE
..SERIAL_PIO_BASE+SERIAL_PIO_LEN).contains(&port))` at the top of each, or a doc line
stating the precondition explicitly. Cheap insurance against a future caller passing a raw
unfiltered port.

### S4 — PIO/MMIO write asymmetry: `pio_write` extends whole slice, `mmio_write` pushes `data[0]`

**Files:** `crates/dh-devices/src/serial.rs:73-75` (`pio_write` →
`out.extend_from_slice(data)`) vs `:99-105` (`mmio_write` → `self.out.push(data[0])`).

A 2-byte `out dx, ax` at 0x3F8 emits **two** bytes to the output buffer (THR+IER on real
hardware, but here both land in the log), while the MMIO THR mirror emits only the low
byte of a 4-byte write. The behaviors are each internally deterministic, so this is not a
correctness bug, but the JSON-log consumer (bead avm: "bytes to the slot's JSON log") sees
a different byte count for "the same" logical write depending on transport. Recommend a
one-line comment on `pio_write` making the "all bytes of a multi-byte OUT are logged"
choice explicit (and ideally a test asserting it), so the asymmetry is a documented
decision rather than an accident. If exact 16550 fidelity is ever wanted, `pio_write`
should transmit only the low byte at THR like the MMIO path — but for an output-only debug
sink, logging all bytes is arguably the more useful behavior; just pin it.

### S5 — 8-byte MMIO write to THR is silently swallowed

**File:** `crates/dh-devices/src/serial.rs:99-105` (`mmio_write`).

The bus permits 8-byte naturally-aligned accesses (`bus.rs:76`: `len == 4 || len == 8`).
`mmio_write` only transmits when `data.len() == 4 && off == 0x08`, so a valid 8-byte write
at offset 0x08 passes the bus check, reaches the device, and is silently dropped (no
transmit). 8-byte reads at 0x08 similarly fall through to `data.fill(0)`. For an
output-only debug device this is acceptable (the mirror is documented as 4-byte slots), but
it's a quiet inconsistency: the bus says "8-byte is legal" and the device says "nothing
happened." A one-line comment ("THR mirror is a 4-byte slot; 8-byte accesses are not part
of the §6.9 mirror and are swallowed/zeroed") would make the boundary intentional.

### S6 — `reg_read` as a free associated fn loses the device's identity

**File:** `crates/dh-devices/src/serial.rs:50-57` (`fn reg_read(reg: u16) -> u8`).

`reg_read` is an associated function (no `&self`) because the model is stateless w.r.t.
register reads — correct today. Worth a one-line note that this is deliberate (it's the
invariant that *makes* the device output-only: register reads can never depend on state),
so a future change that wants `reg_read` to read `self` is forced to confront that it would
be introducing input/state into a device documented as having none.
