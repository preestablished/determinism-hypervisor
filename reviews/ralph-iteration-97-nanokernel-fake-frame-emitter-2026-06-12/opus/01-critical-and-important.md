# Critical & Important Findings

**None.**

No Critical and no Important issues were found in this branch.

The change is additive (+159/-0), follows the established nanokernel guest
pattern exactly, and the load-bearing properties all check out:

- The 4-byte `FRAME_COUNTER` read at entry matches `mmio_read`'s u32 register
  semantics (`crates/dh-devices/src/pad.rs:98-110`).
- `frame_counter` has exactly one writer in the run path — the MMIO write at
  `pad.rs:126-129` — plus `restore` (`pad.rs:152`); `apply_pad_set`
  (`pad.rs:72-80`) touches only `latch`. So the guest's `r10d` and the device
  value cannot diverge, and seeding `r10d` from the device is sound.
- The `'G'` boot marker is emitted exactly once, before the first bump
  (`tests/nanokernel/asm/fake_frames.asm:38-46`).
- The pace loop is byte-identical to `pad_echo`, preserving icount
  determinism across record/replay.
- The drift-pin test correctly enforces register offsets, shared cadence, the
  7-instruction pace body, the boot marker, and the load-bearing read ordering.

The single behavioral edge case (u32 wrap of `FRAME_COUNTER` vs the
strict-increase contract) is genuinely out of practical reach for this guest
and is recorded as a non-blocking note in `02-suggestions.md` rather than
escalated.
