# Action items

### Critical

- [ ] None.

### Important

- [ ] None.

### Suggestions

- [ ] **S1** — (optional) Add a `BOOTINFO_OFF_VERSION == BOOTINFO_VERSION`
  check alongside the magic check in `net_loopback.asm` (~line 51). Note: the
  sibling guests skip this too, so only worth it as a suite-wide hardening pass.
- [ ] **S2** — (optional) Expand the `_BUFFERS_DISJOINT` comment in
  `elf_shape.rs` (~line 519) to note it asserts TX-frame-end ≤ RX-start, not RX
  capacity vs anything mapped above it.
- [ ] **S3** — (optional) Add a one-line comment at `net_loopback.asm`
  `SPIN_BUDGET` (~line 46) stating the budget just needs to exceed worst-case
  exits-until-delivery and the exact value is not load-bearing.
- [ ] **S4** — (optional) Cross-check `NET_LOOPBACK_OK_SEQUENCE` (`b"TRX"`)
  against the asm's three uppercase `putc` sites in the drift test, closing the
  last unpinned guest↔lib.rs gap.

All items are non-blocking. The branch is APPROVE as-is.
