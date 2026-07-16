# Positive notes

### P1 — RX buffer published before the TX doorbell (race-free by construction)

`tests/nanokernel/asm/net_loopback.asm:66-78`

The guest publishes `REG_RX_BUF_GPA` + `REG_RX_CAP` **before** ringing the TX
doorbell, with a comment saying exactly why ("delivery can never race
publication"). Since `apply_net_rx` returns `NoRxBuffer` when `rx_buf_gpa == 0`,
this ordering is what makes the guest robust regardless of how soon run
control's loopback path lands the frame. Correct and well-reasoned.

### P2 — Drift pin checks BOTH sides of the truth, not just one

`tests/nanokernel/tests/elf_shape.rs:462-498`

Register offsets are compared against the *device-side* truth
(`dh_devices::net::REG_*`, `STATUS_OK`, `MAX_FRAME`), while the GPAs and frame
params are compared against the *harness-side* `lib.rs` constants. This is the
right split: the asm must agree with the device it pokes AND with the Rust the
harness reads back. A single-sided pin would let the two drift apart.

### P3 — Const-asserted frame-fits-caps and buffer disjointness

`tests/nanokernel/tests/elf_shape.rs:514-521`

`_FRAME_FITS_DEVICE_CAP`, `_FRAME_FITS_RX_CAP`, and `_BUFFERS_DISJOINT` are
compile-time `const` assertions, so a future edit that bumps `FRAME_LEN` past
`MAX_FRAME`/`RX_CAP`, or moves the buffers into overlap, fails the build rather
than a test run. Catching invariant violations at compile time is the stronger
guarantee.

### P4 — Bounded spin with a loud failure marker

`tests/nanokernel/asm/net_loopback.asm:81-92`

The spin loop is bounded (`rcx = 65536`) and falls through to `.fail_r`
(serial `'r'`) on exhaustion. A guest that hung waiting for a delivery that
never comes would be a miserable failure mode for the M5 acceptance harness;
this one reports a deterministic, diagnosable lowercase byte instead. The
`'T'/'R'/'X'` vs `'t'/'r'/'x'` stage encoding makes "how far did it get" obvious
from the serial log alone.

### P5 — Consistent with the established sibling-guest idiom

The guest reuses the exact `putc` helper, the `.fail_*` → `.fail_out` → `ret`
(crt0 parks in HLT) structure, the uppercase-progress/lowercase-failure serial
convention, and the BootInfo-RAM-floor guard from `device_exercise.asm` /
`capture_fixture.asm`. Following the house style faithfully keeps the suite
uniform and the diff easy to audit.

### P6 — Frame helper mirrors the asm and is itself drift-tested

`tests/nanokernel/src/lib.rs:218-222`, `elf_shape.rs:507-510`

`net_loopback_frame()` recomputes the same `(0x5A + i) & 0xFF` sequence the asm
writes, and the drift test pins `frame[0]`, `frame[63]` (via `wrapping_add`),
and `frame.len()`. The harness (czq) can therefore recompute the expected NET_TX
digest and NET_RX payload from a single source of truth.
