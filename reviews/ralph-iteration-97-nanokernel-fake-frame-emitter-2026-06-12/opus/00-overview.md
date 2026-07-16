# Review: ralph/iteration-97-nanokernel-fake-frame-emitter

- **Branch:** `ralph/iteration-97-nanokernel-fake-frame-emitter` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Stats:** 4 files, +159/-0, 1 commit (`f1707af`)

## Summary

This branch adds `fake_frames`, a new nanokernel test guest (bead r2y) that
serves the M5 `at_frame`/`frame_budget` acceptance. It is a pure fake-frame
emitter: at entry it reads the pv-pad device's absolute `FRAME_COUNTER`
(0x1C) into `r10d`, emits exactly one `'G'` serial boot proof, then loops
forever — incrementing `F`, writing it back to `FRAME_COUNTER` (the
frame-boundary MMIO exit that latches `frame_counter` and logs the AUX
`FRAME_MARK`), and pacing with a fixed `64 × 7`-instruction busy loop cloned
byte-for-byte from `pad_echo`'s pace loop.

The load-bearing design choice — initializing `F` by **reading the device**
rather than starting at zero like `pad_echo` — is correct and well-justified:
because `FRAME_COUNTER` is lineage-ABSOLUTE and survives snapshot/restore via
the PADD section, seeding from the device makes `FRAME_MARK` continuity hold
*by construction* across the snapshot/restore seam, including the
fresh-boot-against-restored-device composition. The 5yo acceptance can then
assert strict increase unconditionally.

Supporting changes are minimal and idiomatic to this tree: a `PROGRAMS` entry
in `build.rs`, an accessor + `FAKE_FRAMES_BOOT_MARKER` / `FAKE_FRAMES_PACE_ITERS`
constants in `lib.rs`, ELF-shape registration, and a drift-pin test that
mirrors the established `pad_echo`/`net_loopback` pattern — additionally
pinning the load-bearing `FRAME_COUNTER` read *before* the `.frame` loop using
a comment-stripped positional check so the doc header cannot satisfy the pin.

## Verification performed

- **MMIO read width/semantics:** `mov r10d, [r8 + REG_FRAME]` is a 4-byte read
  into a 32-bit register; `mmio_read` returns `self.frame_counter` (u32) only
  for `data.len() == 4`. Width and semantics correct.
- **Sole-writer / no divergence:** `frame_counter` is mutated only by the
  MMIO write at 0x1C and by `restore`. `apply_pad_set` touches only `latch`.
  Within a run the guest is the only writer, so `r10d` and the device value
  cannot diverge.
- **Boot marker:** `'G'` is emitted exactly once, after the read and before
  the first bump — a harness can sync on it.
- **Pace-loop equivalence:** the `.pace` body is identical to `pad_echo`
  (same 7 instructions, same seed `0x9AD5`, same `PACE_ITERS=64`), preserving
  icount determinism. The drift test pins both the shared cadence and the
  7-instruction body.
- **Drift-pin robustness:** comment-stripping + positional ordering are sound;
  the parser counts the pace body to exactly 7.

## Verdict

**APPROVE**

No Critical or Important issues. One worth-noting non-blocking item (u32
`FRAME_COUNTER` wrap vs the strict-increase contract) and a couple of minor
suggestions, all documented in the following files.
