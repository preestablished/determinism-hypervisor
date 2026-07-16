# Review Overview — fake_frames emitter guest

- **Branch:** `ralph/iteration-97-nanokernel-fake-frame-emitter` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 4 files, +159 / -0, 1 commit (`f1707af`)

## What changed

A new nanokernel test guest, `fake_frames`, plus its plumbing:

- `tests/nanokernel/asm/fake_frames.asm` (+68) — a pure fake-frame emitter. At
  entry it **reads** the pv-pad `FRAME_COUNTER` (offset `0x1C`) into `r10d`,
  emits a single `'G'` serial byte (boot proof), then loops forever:
  bump `FRAME_COUNTER` by 1 (the frame-boundary MMIO write the host logs as an
  AUX `FRAME_MARK`), run a fixed `PACE_ITERS=64` × 7-instruction busy loop, repeat.
  No pad polling, no RAM observation table — the host-side FRAME_MARK table is
  the entire observable.
- `tests/nanokernel/src/lib.rs` (+20) — `fake_frames_elf()`, the
  `FAKE_FRAMES_BOOT_MARKER = b'G'` and `FAKE_FRAMES_PACE_ITERS = 64` constants,
  plus a non-empty assertion in the existing all-guests test.
- `tests/nanokernel/build.rs` (+1) — registers `fake_frames` in `PROGRAMS`.
- `tests/nanokernel/tests/elf_shape.rs` (+70) — a drift pin
  (`fake_frames_asm_matches_rust_constants`) checking the device offsets, the
  shared pace cadence, the **presence and ordering** of the load-bearing
  `FRAME_COUNTER` read, and the 7-instruction pace-loop body; plus the guest in
  the shared `every_guest_is_a_static_x86_64_exec_at_the_load_addr` shape test.

## Summary judgment

This is a clean, small, well-disciplined addition that closely mirrors the
already-accepted `pad_echo` guest (bead 29a) and reuses its proven pace loop and
drift-pin idiom. The load-bearing novelty — initializing `F` by reading the
device counter rather than from zero — is correctly implemented and, notably,
**guarded by an ordering assertion in the drift pin** so a future edit that
drops or reorders the read fails loudly. I verified against `runctl.rs` that
`frame_budget` counts FRAME_COUNTER **writes** since run start, so the extra
boot-time **read** exit does not perturb frame-budget semantics, and that the
§6.6 ring-W FrameMark equality rule only fires for channel-attached guests — a
channelless emitter like this (same as pad_echo) is not at risk of being faulted.

I found **no Critical or Important issues**. The asm, constants, and pin are
internally consistent and consistent with `pad.rs`, `pad_echo.asm`, `crt0.asm`,
and ARCHITECTURE §6.4/§6.6. The only substantive remarks are documentation
tightening (the doc header slightly overstates the "fresh-booted guest against a
restored device" composition) and minor maintainability/comment nits, all
non-blocking.

## Verdict

**APPROVE.** 0 Critical, 0 Important, 4 Suggestions.
