# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **S1 — Document the u32 `FRAME_COUNTER` wrap boundary.** Add one line
  (asm header in `tests/nanokernel/asm/fake_frames.asm`, or wherever the M5
  acceptance documents the invariant) noting that strict-increase holds only
  below 2^32 frames and that the wrap is out of practical reach for this
  guest. Relevant: `fake_frames.asm:42-43`, `crates/dh-devices/src/pad.rs:51,126-129`.
  No code change. Non-blocking.

- [ ] **S2 — Clarify the inherited `and ebx, 511` / `work_buf` comment.**
  In `tests/nanokernel/asm/fake_frames.asm:51-53`, optionally note the pace
  body is carried unchanged from `pad_echo` so the shared-cadence intent is
  explicit. Cosmetic. Non-blocking.

- [ ] **S3 — (Optional) Pin `work_buf` size vs the `and` mask.** In
  `tests/nanokernel/tests/elf_shape.rs`, optionally assert
  `resq 512` ↔ `and ebx, 511` so a future `PACE_ITERS` retune past the mask is
  caught. Low value; safe to skip.

---

**Verdict: APPROVE.** No Critical or Important items. All suggestions are
non-blocking and may be deferred or skipped at the author's discretion.
