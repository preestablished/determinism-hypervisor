# Suggestions (non-blocking)

### S1 — BootInfo check omits the `version` field (minor inconsistency)

`tests/nanokernel/asm/net_loopback.asm:48-54`

The guest checks `BOOTINFO_OFF_MAGIC == BOOTINFO_MAGIC` but not
`BOOTINFO_OFF_VERSION == BOOTINFO_VERSION`. The sibling guests are also
inconsistent here (`capture_fixture.asm` checks magic only too), so this is not
a regression — just noting that neither validates version. If a future BootInfo
v2 ever reshuffles offsets, magic alone won't catch it. Low value; skip unless
you're hardening the whole guest suite at once.

### S2 — The const-assert comment slightly understates what it proves

`tests/nanokernel/tests/elf_shape.rs:519-521`

```rust
// Buffers must not overlap each other (TX cap is the frame itself).
const _BUFFERS_DISJOINT: () = assert!(
    NET_LOOPBACK_TX_GPA + NET_LOOPBACK_FRAME_LEN as u64 <= NET_LOOPBACK_RX_GPA
);
```

This asserts TX-frame-end ≤ RX-start, which is correct. Worth a one-line note
that it does **not** assert the RX buffer's *capacity* (2048 bytes) stays clear
of anything above it — fine here because nothing is mapped above RX, but if a
later guest adds a third buffer the disjointness invariant will need extending.
Purely documentary.

### S3 — Spin budget is a magic constant without a stated lower bound

`tests/nanokernel/asm/net_loopback.asm:46`, `:88`

`SPIN_BUDGET = 65536` is a generous ceiling, which is the right call for a
"loud `r` instead of hang" safety net. The module doc explains *why* the spin
is bounded but not *why 65536* specifically (vs, say, 256). Since the loopback
re-lands the frame at a future icount determined by run control, the budget
just needs to exceed the worst-case exits-until-delivery. A brief comment
("budget >> worst-case delivery latency; exact value not load-bearing") would
save the next reader from wondering whether the number is tuned. Optional.

### S4 — Consider asserting `NET_LOOPBACK_OK_SEQUENCE` length/content in the drift test

`tests/nanokernel/src/lib.rs:208`, `tests/nanokernel/tests/elf_shape.rs`

`NET_LOOPBACK_OK_SEQUENCE = b"TRX"` is the harness's success oracle but isn't
cross-checked against the asm's three `putc 'T'/'R'/'X'` sites. A tiny grep-style
assertion (the asm emits exactly these three uppercase bytes on the success
path) would close the last unpinned gap between the guest and its lib.rs
contract. Low priority — the bytes are simple and stable.
