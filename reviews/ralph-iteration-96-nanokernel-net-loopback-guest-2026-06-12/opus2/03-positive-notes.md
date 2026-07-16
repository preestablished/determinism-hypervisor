# Positive notes (specific)

## The RX-before-TX ordering is real, not just documented
The header claims "RX buffer published BEFORE the TX doorbell so delivery can
never race publication," and the asm actually does it: `REG_RX_BUF_GPA` /
`REG_RX_CAP` writes (`net_loopback.asm:79-81`) precede the TX programming and
doorbell (`:86-89`). Many guests get this backwards in the comment vs the code;
this one is consistent.

## The drift pin gates on the *device-side* truth, not a local copy
`elf_shape.rs:474-481` compares the register-offset `%define`s against
`dh_devices::net::REG_*` directly — so if the device ABI ever moves a register,
this guest's test fails loudly rather than the guest silently poking a stale
offset. Pinning `STATUS_OK` as a value (via `u32::try_from`) separately from the
offsets shows the author understood the value-vs-offset distinction.

## Compile-time fit/disjoint asserts are the right tool
`elf_shape.rs:518-525` uses `const _: () = assert!(...)` to prove
`FRAME_LEN <= MAX_FRAME`, `FRAME_LEN <= RX_CAP`, and
`TX_GPA + FRAME_LEN <= RX_GPA` at compile time. These are exactly the invariants
that would otherwise rot silently if someone tweaked a constant — encoding them
as compile errors (not runtime assertions that need the test to run) is the
durable choice. The buffer-disjointness check in particular pre-empts a
nasty class of TX/RX aliasing bug.

## Bounded spin with a distinct failure letter
The `loop`-bounded poll (`:95-100`) with a dedicated `'r'` on exhaustion means a
broken or absent harness produces a loud, greppable failure instead of a hang —
a genuinely better failure mode than the obvious `jnz`-forever loop, and it
correctly relies on `RX_LEN` stickiness so it can't false-negative.

## Faithful to the device contract's subtle rules
The guest's flow honors every contract in the `net.rs` module doc: it gates the
post-TX progress on `TX_STATUS == STATUS_OK` (the iteration-86 critical that
consumers must gate on OK), it clears `RX_LEN` itself (the "guest's job"
consumer-clears contract), and it relies on exactly the sticky-RX_LEN /
single-deep-register semantics the device actually provides.

## putc matches the established convention exactly
`putc` (`:135-138`, "Clobbers DX only") is byte-for-byte the same helper and
comment as `capture_fixture.asm:206-210`, keeping the nanokernel guest family
consistent rather than inventing a new serial idiom.
