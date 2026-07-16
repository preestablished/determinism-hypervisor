# Suggestions

### S1 — smoke serves zeros for the pv-clock read; 997 is path-independent but the smoke doesn't prove that

- Location: `tests/determinism/tests/counting_smoke.rs:88-92` (the
  `VcpuExit::MmioRead` arm fills `0`).
- The real pv-clock device (`crates/dh-devices/src/clock.rs`: `REG_VNS` at
  0x08 returns `vns_base + f(icount)`, monotone) returns nonzero, icount-
  dependent data through the M1 device-bus run loop — a *different* code path
  from the smoke's raw zero-fill. The 997 count is in fact independent of the
  returned value: the MMIO read instruction retires zero regardless of data,
  and the loaded value lands in `rax` which `xor rax,rax` immediately discards
  two instructions later. So 997 will hold under the real bus too. But the
  smoke can't *demonstrate* that, and `gfb`'s description ("against the
  completed M1 device surface") implies the M2 acceptance should run on the
  real bus. Suggest a one-line comment in the smoke noting that 997 is a
  retirement-semantics property, not a device-path property, so the M2
  single-step test (gfb) can reuse the figure even though it drives the real
  pv-clock; and that gfb is where the bus-loop path gets validated.

### S2 — the dual-marker-in-one-exit comment the prompt asked about

- Location: `counting_smoke.rs:71-77` (the `for &b in data.iter()` loop).
- The guest uses single-byte `out dx, al`, so 'S' and 'E' can never share one
  IoOut. But if they ever did, both `at_s` and `at_e` would latch the SAME
  `now`, silently yielding delta 0. Harmless today, but a future edit that
  batches markers (e.g. `rep outsb`) would make this fail confusingly. A
  one-line comment ("single-byte OUTs only; batching would alias the two
  latches") would future-proof it. Low priority.

### S3 — `counting.asm` `.never` label fall-through is subtle

- Location: `tests/nanokernel/asm/counting.asm:75-78` (`jne .never` /
  `.never: ret`).
- The not-taken `jne .never` is meant to fall through into the loop, and the
  `.never` label is reused as the `ret` target after the E-OUT. The
  disassembly confirms the E-OUT (`out %al,(%dx)` at 100fb2) sits immediately
  before `.never: ret` (100fb3), so control reaches `ret` by fall-through from
  the E-OUT, not via the branch. This is correct but relies on the reader
  noticing the label does double duty. A two-word comment on `.never`
  ("also the post-E fall-through target") would help the next editor avoid
  accidentally inserting an instruction between the OUT and the ret.

### S4 — magic GPA `0xD000_6008` duplicated across asm, lib.rs, and the smoke

- Location: `counting.asm` (`MMIO_BASE + SERIAL_THR` = 0xD0006008),
  `counting_smoke.rs:84` (literal `0xD000_6008`).
- The asm derives it from `%define`s; the smoke hardcodes the literal. They
  agree today, but there is no drift test tying the smoke's literal to the
  asm's composition (unlike the existing bootinfo.inc drift test). Consider a
  `COUNTING_MMIO_THR_GPA` constant in `lib.rs` consumed by the smoke, so a
  future MMIO_BASE change can't silently desync the test's filter. Minor.
