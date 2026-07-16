# Positive Notes

## P1 — The drift pin guards the *intent*, not just the constants

`elf_shape.rs:fake_frames_asm_matches_rust_constants` goes beyond the usual
offset/cadence mirroring: it strips comments first (so the doc header can't
satisfy the read pin), asserts the load-bearing `FRAME_COUNTER` read **exists**
and **precedes** the `.frame` loop (`read_at < loop_at`), and counts the
7-instruction pace body between `.pace:` and its `jnz`. This is exactly the kind
of test that survives a careless future edit — a register rename or a dropped
read fails loudly with a descriptive `expect()` message rather than silently
weakening continuity. Genuinely good defensive testing for hand-written asm.

## P2 — Correct, minimal reuse of the proven pad_echo cadence

The pace loop is byte-for-byte the `pad_echo` body, and the constant is pinned
equal to `PAD_ECHO_PACE_ITERS` with an explicit
`assert_eq!(FAKE_FRAMES_PACE_ITERS, PAD_ECHO_PACE_ITERS, "shared cadence")`.
That keeps the two M5 guests on an identical, already-validated frame cadence and
makes any future divergence a compile-time test failure. The `7-instruction body`
pin is mirrored in the pad-echo pin too, so the shared invariant is protected on
both sides.

## P3 — The read-init design is the *right* call and is explained

Initializing `F` from the device counter rather than zero is the correct way to
make strict-increase hold across the snapshot/restore seam without relying on the
acceptance to reason about whether the guest register carried `F`. The header
correctly identifies this as "THE LOAD-BEARING DIFFERENCE from pad_echo" and the
pin enforces it. Even granting the small overstatement in S1, the underlying
engineering instinct — make the invariant hold *by construction* at the device,
the single source of truth — is sound.

## P4 — Honest scoping: one observable, no incidental state

The guest deliberately does *nothing* but make frames: one `'G'` boot byte, then
silent; no RAM table, no pad polling, no IDT/STI. That keeps the FRAME_MARK table
the sole observable and removes any chance of an incidental RAM/serial side
effect muddying the M5 at_frame/frame_budget acceptance. The header states this
explicitly ("this guest exists to make frames, nothing else"), and `lib.rs`
documents `FAKE_FRAMES_BOOT_MARKER` as a single byte (a `u8`, not an OK-sequence)
— consistent with a one-byte boot proof rather than a multi-byte handshake. I
confirmed `'G'` is the only `out` of an uppercase letter in the guest.

## P5 — `.bss` placement needs no GPA const-assert

Unlike `pad_echo`'s `TABLE_GPA` (a fixed 0x30_0000 the host reads, correctly
const-asserted not to collide with the 0x40_0000 channel), `fake_frames`'
`work_buf` is a linker-placed `.bss` symbol with no host-side GPA contract — so
there is correctly *no* GPA assertion to add. The author resisted the temptation
to copy `pad_echo`'s table machinery wholesale; the guest carries only what it
needs.
