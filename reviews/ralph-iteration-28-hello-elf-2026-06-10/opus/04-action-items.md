# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **S1 — Correct stale plan wording (follow-up bead, not this PR).** Update
  `IMPLEMENTATION-PLAN.md:18` so M0 no longer says "real-mode→long-mode stub"; it contradicts
  ARCHITECTURE.md §2.3 (long mode entered directly via `KVM_SET_SREGS`). The asm header
  already documents the discrepancy; fixing the plan removes it at the source.
- [ ] **S2 — Self-describe the string length** in `tests/nanokernel/asm/hello.asm`:
  `MSG_LEN equ $ - msg` instead of `%define MSG_LEN 6`, so the length cannot drift from the
  literal if the message is edited.
- [ ] **S3 — (Optional) cheap host-side serial assertion** scanning the emitted `.rodata`
  for `HELLO_SERIAL_OUTPUT`, if it's low-cost; the full boot assertion remains in bead 1mz.

**Merge recommendation:** approve and merge as-is. No action item blocks this PR.
