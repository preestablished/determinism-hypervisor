# Action Items

### Critical

- [ ] None.

### Important

- [ ] None.

### Suggestions (all optional, non-blocking)

- [ ] **S1 — Tighten the restore-composition rationale.** In
  `tests/nanokernel/asm/fake_frames.asm:8-12` (and the mirrored prose in
  `tests/nanokernel/src/lib.rs:228-233` and `elf_shape.rs:550-554`), soften
  "BY CONSTRUCTION for every composition — including a fresh-booted guest against
  a restored device state." The normal `RestoreSnapshot` path restores guest
  registers and the pv-pad section together (`pad.rs::restore` reloads
  `frame_counter`), so a fresh-guest/restored-device pairing is not what the
  default path produces — it needs a harness that pre-seeds the device counter.
  Reframe as defense-in-depth + harness flexibility (keep the rationale, don't
  remove it). Suggested wording is in `02-suggestions.md` S1.

- [ ] **S2 — Add a half-line wrap note.** Near the bump at
  `tests/nanokernel/asm/fake_frames.asm:32-33`, note that the `u32`
  `FRAME_COUNTER` wraps at ~2^32 frames (~2e12 instructions), which is
  unreachable, so strict-increase holds in practice and no wrap handling is
  needed.

- [ ] **S3 — (Conditional) strengthen the read-ordering pin.** If "the `'G'`
  boot proof must come *after* the read" is load-bearing, add an assertion in
  `tests/nanokernel/tests/elf_shape.rs` (~line 587-595) that the read precedes the
  `'G'` OUT, not just `.frame:`. If it is not load-bearing, leave the existing
  `read_at < loop_at` as-is.

- [ ] **S4 — Clarify the `and ebx, 511` comment.** In
  `tests/nanokernel/asm/fake_frames.asm:38-40`, note that here the pace loop is
  the guest's *only* memory writer, so the mask is the sole bound on `work_buf`
  if `PACE_ITERS` is ever retuned past 512.

### Verdict

APPROVE — merge as-is. The suggestions above are documentation/comment polish and
an optional test strengthening; none block the merge.
