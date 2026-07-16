# Suggestions (non-blocking)

## S1 — u32 `FRAME_COUNTER` wrap silently breaks strict-increase

**Where:** `tests/nanokernel/asm/fake_frames.asm:42-43` (`add r10d, 1` then
write-back) against `crates/dh-devices/src/pad.rs:51,126-129`
(`frame_counter: u32`, `self.frame_counter = value`).

`FRAME_COUNTER` is a u32 and the device write just latches `value` with no
saturation or wrap detection. The guest computes `add r10d, 1`, which wraps
mod 2^32, so at `F = u32::MAX` the next bump writes `0` — a *decrease*. The
module docs describe `FRAME_COUNTER` as "strictly increasing along a lineage"
and the 5yo acceptance asserts exactly that.

In practice this is unreachable for `fake_frames` (2^32 frames at this pacing
is years of continuous execution), so it is **not** a blocker. But since this
guest's entire reason for existing is to exercise the strict-increase property
across the snapshot/restore seam, it is worth one sentence acknowledging the
wrap boundary — either:

- a comment in the asm header noting that strict-increase holds only below
  `2^32` frames and that the wrap is out of practical reach, or
- a brief note wherever the M5 acceptance documents the invariant, so a future
  reader stress-testing with a high seeded `frame_counter` (e.g. restoring a
  PADD with `frame_counter` near `u32::MAX` to probe the seam) isn't surprised.

No code change required; this is a documentation/clarity nicety.

## S2 — `and ebx, 511` / `work_buf` comment is inherited but slightly stale here

**Where:** `tests/nanokernel/asm/fake_frames.asm:51-53`.

The inline comment ("`and ebx, 511` bounds work_buf writes if pacing is ever
retuned past 512") is copied verbatim from `pad_echo`, where the pace loop
shares a function with the table-append logic. In `fake_frames` the pace loop
is the *only* consumer of `work_buf` and the comment is accurate, so this is
purely cosmetic — but since the surrounding prose was otherwise rewritten for
this guest, a one-line tweak noting "carried from pad_echo's pace loop
unchanged" would make the lineage explicit and reduce the chance a future
edit diverges the two bodies without noticing the drift pin protects them.

## S3 — Drift test could assert `work_buf` size matches the `and` mask

**Where:** `tests/nanokernel/tests/elf_shape.rs:551-621`.

The test pins the 7-instruction pace body and the cadence, but not the
`work_buf: resq 512` / `and ebx, 511` relationship that keeps the busy-loop
stores in-bounds. This is low value (the store is bounded for any
`PACE_ITERS <= 512` regardless), so only worth it if you want the pin to also
catch a future retune that pushes `PACE_ITERS` past the mask. Listed for
completeness; safe to skip.
