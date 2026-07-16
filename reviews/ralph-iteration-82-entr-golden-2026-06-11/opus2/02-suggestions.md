# Suggestions

### S1. Assert `outcome.cumulative_icount == a1.boundary.icount` after restore

**Where:** `entr_golden.rs:296-307` — `outcome.cumulative_icount` is captured but never
read.

`restore_snapshot` returns `cumulative_icount = time.cumulative_icount`
(restore_engine.rs:356), which was written as `boundary.icount` at snapshot
(snapshot_engine.rs:232 — `cumulative_icount: boundary.icount`). So the snapshot's
boundary icount survives the round trip and is *available* for free. The module doc
correctly explains that absolute icounts diverge between legs *after* restore (continuous
counter axis, `counter=None`), so chain values aren't compared — but the snapshot boundary
icount itself is a fixed, comparable quantity. Adding
`assert_eq!(outcome.cumulative_icount, a1.boundary.icount)` strengthens the round-trip claim
(the §3.1 accounting field is preserved, not just the entropy tuple) at zero cost and
documents that the field is intentionally inspected.

### S2. The fault path's `0xDEAD` LEN poison is dead and worth a one-line comment

**Where:** `entropy_draw.asm:429-433`.

`.fault` writes `0xDEAD` (57005) to `REG_LEN`, then HLT-spins forever without ringing the
doorbell again. Two notes:

- The write is **inert** — no subsequent doorbell consumes the new LEN, so the "57005-byte
  fill clobbers guest RAM" scenario the prompt flags **cannot** occur here (it would only
  bite a guest that re-rang after fault; and even then `0xDEAD < MAX_FILL = 1<<20`, so the
  device would *succeed* with a 57KB fill rather than fault — a real footgun for any future
  fault-then-retry guest). For *this* guest it is harmless but pure decoration.
- The actual observable poison is that `count` is **not** bumped, so `count_pause` falls
  short and `assert_eq!(count_pause, BATCHES_BEFORE * ENTROPY_DRAW_BATCH)`
  (entr_golden.rs:254) trips. That exact-count assert **is** sufficient to detect a fault —
  the harness never reaching the expected count is an unambiguous failure.

Recommendation: either (a) drop the `mov dword [r8+REG_LEN], 0xDEAD` and just HLT-spin
(the count shortfall already fails the harness), or (b) keep it but add a comment that it
is a *human-debuggable* marker for `gdb`/memory dumps only, not a harness-checked signal —
the current comment "poison: harness count check trips" slightly overstates its role (the
count shortfall, not the LEN write, is what trips the harness). I lean toward (b) with the
clarified comment; the LEN marker is genuinely handy when staring at a hung guest.

### S3. Add a defensive `assert` that the restored slot's STATUS register is OK / not faulted

Leg B's correctness depends on the device re-issuing OK fills. If a future change broke the
device re-registration or LEN re-programming on restore, leg B's draws would silently come
out as zeros or diverge — caught by the byte compare, but again only as a downstream
mismatch. Since the guest itself jumps to `.fault` on `STATUS != OK`, the harness's
`GuestHalted`-only assertion (entr_golden.rs:206-210) already catches a fault as an
"unexpected batch boundary." This is adequately covered; flagging only so the reviewer
trail records that the STATUS path is guarded by the guest, not the harness.

### S4. `VmMem` is duplicated across crates (entr_golden.rs and m1_acceptance.rs)

**Where:** `entr_golden.rs:73-87` vs `tests/determinism/tests/m1_acceptance.rs` (same
`GuestMem` adapter). These live in **different crates** (`dh-worker` vs `determinism`), so
sharing would require a published helper crate or a `pub` adapter in `dh-devices` — not
worth it for a ~12-line struct. The duplication is acceptable per the research note's
"deduplicate fixtures *within* a crate" framing; cross-crate test helpers are a known Rust
friction point. **Do not** move `VmMem` into `tests/common/mod.rs`: no other `dh-worker`
test needs it, so it would be dead weight compiled into every test binary in that
directory. Leave as-is.

### S5. `fresh_log()` / `config()` are also duplicated vs m1_acceptance — same verdict

Same cross-crate boundary as S4. Acceptable. If a third `dh-worker` test ever needs them,
*then* promote `fresh_log`/`config` into `tests/common/mod.rs` (they're `dh-worker`-local,
so that move is clean — unlike `VmMem`). No action now.

### S6. Consider one extra batch before the snapshot to harden against an off-by-one in "mid-stream"

The snapshot is taken after exactly `BATCHES_BEFORE = 2` batches (512 draws), with the
guest parked at a clean HLT boundary. That is genuinely *mid-stream* for the PRNG
(`word_pos` is non-trivial). No change required — just noting the snapshot is deliberately
at a batch boundary, not mid-batch, which is the *only* boundary where the guest is
stoppable without a landing. A truly mid-*batch* snapshot is impossible by construction
here (no exit between draws except the doorbell MMIO), and the module doc is honest that
this is batch-granular. Fine as designed.
