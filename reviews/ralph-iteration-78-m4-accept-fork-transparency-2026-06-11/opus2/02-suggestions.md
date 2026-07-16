# Suggestions

### S1 — Add a divergence-sensitivity guard so the equality can be SEEN to fail

`frozen_parent_children_replay_identical_inputs_identically` proves
`out_a == out_b` under the *same* inputs X. The non-vacuity pins that exist
are good (`injections_delivered == 3`, `count_a == 3`,
`vec_a == [0x40, 0x41, 0x40]`) — they guarantee X actually landed and was
recorded, so the equality is not trivially `0 == 0`.

What is NOT proven is that the equality is *input-sensitive*: that a
DIFFERENT input set produces a DIFFERENT outcome. One more fork — a child C
with inputs Y (e.g. vectors reordered `[0x41, 0x40, 0x40]`, or different
icounts) and a single `assert_ne!(out_c.state_hash, out_a.state_hash)` —
would prove the whole rig can fail, i.e. that the chain genuinely reflects
the injected vectors and isn't blind to them. This is the single most
valuable cheap strengthening: it turns "A and B agree" into "A and B agree
*and the test would notice if they didn't*." Worth one extra fork.

(`crates/dh-worker/tests/fork_engine.rs::second_child_sees_the_pristine_parent_after_first_child_diverged`
already proves CoW byte-isolation under divergence at the memory level, so
the gap here is specifically *running-guest divergence visible through the
chain*, which no existing test covers.)

### S2 — Assert the parent is still pristine after both children ran

The parent ran with `&[]` (no injections), so its ISR table at
`TIMER_GUEST_TABLE_GPA` should hold count 0 even after A and B forked off
it and recorded into THEIR CoW copies. A two-line read-back of the parent's
`guest_mem` table asserting `count == 0` would prove, in *this* running
context, that the children's recorded deliveries never bled back into the
frozen parent — the running-guest analogue of fork_engine.rs's byte-isolation
test. Cheap and directly on-point for "fork transparency."

### S3 — Capture-equality beyond the hash for A vs B

`assert_eq!(out_a, out_b)` already compares `state_hash` (a full RAM+vCPU
walk that subsumes the ISR table), so the explicit `(count_a, &vec_a) ==
(count_b, &vec_b)` line (m4_transparency.rs:409) is technically redundant
with the hash. It is, however, a *good* redundancy: a hash mismatch is
opaque, whereas a `[0x40,0x41,0x40] != [0x40,0x41]` diff names the failure.
Keep it — but consider a comment marking it as the human-readable
cross-check so a future reader doesn't "simplify" it away thinking it's
dead weight.

### S4 — `ITERS_CMDLINE` is now a misleading name for the timer guest

See I1. The constant name and its doc-comment describe a landing-loop
iteration count (`8 instructions each; 30M iters = 2.4e8 capacity`). For the
timer guest the same bytes are a mode-select string whose leading digit
means "default mode." Either decouple the two (S1 in I1) or rename/comment
so the dual role is visible.

### S5 — `count as usize` Vec sizing reads guest-controlled length unbounded

`run_child` reads a `u64` count from guest RAM and uses it directly as a
`Vec` size (`vec![0u8; count as usize]`). For trusted test guests this is
fine, and the subsequent `assert_eq!(count_a, 3)` would catch a wild value
*after* allocation. If the guest ever miswrote a huge count (e.g. a guest
bug, or reading an uninitialized table on a future guest), the test would
OOM-abort rather than fail cleanly. A `assert!(count <= SOME_SANE_CAP)`
before the allocation (or asserting `count == 3` before reading the vector
slice) would make a guest-table corruption a clean test failure instead of
an allocator abort. Low priority; defensive only.

### S6 — Maintainability: the file is fine at 3 tests / 410 lines, no split needed

The prompt floated splitting `m4_transparency.rs`. At 410 lines and 3
`#[test]`s, all sharing the `boot`/`run_more`/`config` fixtures and the same
H1==H2 milestone narrative, this is cohesive — and the research note warns
that each integration-test FILE adds a link step and recommends grouping
related assertions into one file. Keep it as one file. The shared helpers
(`boot`, `run_more`) are already deduplicated, which is the right call.
