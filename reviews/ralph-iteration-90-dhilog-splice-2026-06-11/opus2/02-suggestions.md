# Suggestions

## S1 — Boot-rooted lineage (`root_base() == [0;32]`) is legal-by-design but silent; document it and consider an `is_boot_rooted()` helper

`root_base()` (`splice.rs:130`) returns the first segment's `base_snapshot_id` verbatim,
which may be all-zeros for a boot segment (a recording that starts from cold boot, not a
restored snapshot). The mid-lineage zero base is structurally precluded (see 01 analysis
(d)), but the *root* zero base is the legitimate boot case.

The consumer (cw2) will hand `root_base()` to the store as a `SnapshotRef::from_bytes(...)`
(see S2). A zero ref is `SnapshotRef::zero()` — a real, distinguished value in
`snapstore-types` (`zero()` exists at lib.rs:47). cw2's M7 harness is "root snapshot →
1000 forks", so its root is a real snapshot, not zero — but a future caller replaying a
boot-rooted lineage needs to know `root_base()==0` means "start from cold boot, do NOT
restore a snapshot."

**Suggestion:** add a one-line doc on `root_base()` ("zeros ⇒ boot-rooted: replay starts
from cold boot, not a restored snapshot") and optionally an `is_boot_rooted(&self) -> bool`
returning `self.root_base() == [0u8; 32]`. This makes the boot case a named property rather
than a magic-value the consumer has to recognize.

## S2 — `edges()` yields `[u8;32]`, but the consumer (`verify_replay`) takes `SnapshotRef` — note the boundary

`Edge::base_snapshot_id` is a raw `[u8;32]` (:51). The consumer
`dh-worker::verify_replay::verify_replay` takes `base_snapshot: SnapshotRef`
(`verify_replay.rs:33`), and `SnapshotRef(pub [u8;32])` with `from_bytes` exists
(`snapstore-types/src/lib.rs:44,55`). So the mapping is `SnapshotRef::from_bytes(edge.
base_snapshot_id)` — clean, one call. No change required, but the doc on `edges()` ("replay
`edge.log` from `edge.base_snapshot_id`") could name the conversion so the harness author
doesn't wonder whether splice should depend on `snapstore-types` (it correctly should NOT —
keeping splice free of the store type is right; this is just a doc breadcrumb).

## S3 — Mid-lineage non-zero `entropy_seed` silently re-seeds; expose it so cw2 can audit "seeded" children

`replay_engine.rs:134` honors `header.entropy_seed`: zeros ⇒ continue the base snapshot's
PRNG; non-zero ⇒ re-seed fresh from it. A `Lineage` carries the per-segment headers but
exposes nothing about the entropy axis. cw2's children are described as **"seeded pad-burst
children"** — each child legitimately re-seeds to get a *distinct* random burst. That is the
correct, intended use of a non-zero `entropy_seed` per child segment.

The splice layer's silence is defensible (re-seeding is a replay-honored header fact, not a
continuity violation — it does not break the stitch). But for cw2's audit ("each child runs
a DISTINCT seeded burst → 1000/1000 VerifyDone"), being able to read which edges re-seed,
and with what seed, would let the harness assert seed-distinctness *before* replay, catching
a harness bug (two children accidentally sharing a seed → identical bursts → identical refs,
which would silently pass VerifyReplay but defeat the *purpose* of 1000 distinct forks).

**Suggestion:** add `Edge::entropy_seed: [u8;32]` (or `reseeds: bool` plus a `seed()`),
populated from the header. Free to add, and it turns "distinct seeds" from a harness
assumption into a checkable invariant. Low priority but a genuine fit for the named M7
consumer.

## S4 — `end_identity()` returns `(end_snapshot_id, end_state_hash, end_icount)` — covers VerifyDone; confirm `end_vns` is not needed at the lineage boundary

`VerifyProgress::Done { total_icount, end_state_hash }` (`verify.rs:20`) is what cw2's
acceptance checks ("matching end_state_hash"). `verify_replay` builds it from the
*per-segment* replay outcome (`verify_replay.rs:76`), and `end_vns` is verified
*internally* per segment by the replay engine (`replay_engine.rs:326`) — it never reaches
the cross-segment boundary. So `end_identity()` correctly omits `end_vns`: VerifyDone does
not carry it, and it is a per-segment internal check.

The two fields cw2 needs at the *lineage* boundary are both present: `end_state_hash` (the
acceptance comparand) and `end_snapshot_id` (the leaf snapshot ref that "TakeSnapshot each"
produces, used by the cross-check bead `dsg` to re-run from root on a different slot and
compare refs). `end_icount` is a bonus. **No change — recording the confirmation so the
omission reads as deliberate, not an oversight.**

## S5 — Test gap: no single leaf-only / empty segment lineage (the simplest cw2 child case is untested)

The dhilog writer proves a sealed log with ZERO records besides END is legal
(`dhilog.rs:581` `empty_log_is_just_header_plus_end`). `Lineage` never inspects record count
— it only reads header fields — so a single empty/leaf-only segment validates fine. But
there is **no splice test** for:

- `Lineage::new(&[leaf_only])` where the single segment has a non-trivial body — actually
  the existing tests always have `pad_set(1000, ...)` before seal, so the empty-body case
  (END-only) is untested at the splice layer. A cw2 child whose pad burst is empty (edge
  case) or whose only content is the burst is the literal common case.
- The simplest cw2 child shape: ONE root snapshot, ONE child segment ⇒ a two-segment
  lineage where the leaf has zero end snapshot. `extend_is_the_fork_composition` covers the
  2-segment `extend` path, but not the direct `new(&[root, child])` leaf case asserting the
  edges/end_identity for it.

**Suggestion:** add a test `single_leaf_only_segment_validates` building `seal_segment`
with no records (just seal) and asserting `Lineage::new(&[&leaf]).unwrap()` gives `len()==1`,
`root_base()==base`, `end_identity().0 == [0;32]`, and a single edge. Cheap, and it pins the
exact shape M7 leans on.

## S6 — Test gap: corrupt-segment test flips the LAST byte (body-hash path only); also flip a HEADER byte

`continuity_violations_are_loud` (:286) flips `corrupt[last] ^= 0xFF` — the last body byte,
caught by `BodyHashMismatch`. That exercises one `ReadError` path. A hostile lineage can also
present a structurally-broken *header* (bad magic, unsealed flag, reserved-nonzero,
zero clock). The splice layer maps ALL of these to `SpliceError::Segment { index, err }`
uniformly, but the test only proves the body-hash path round-trips the index.

**Suggestion:** add a second corruption flipping a header byte (e.g. clear the SEALED flag
at byte 12, or corrupt the magic at byte 0) and assert `SpliceError::Segment { index: 1,
err: ReadError::NotSealed }` (or `BadMagic`). This proves the unsealed-log rejection — the
single most important reader guarantee for a replay-feeding splice — actually propagates
through `Lineage`. (The reader's own tests cover `NotSealed`; this proves splice does not
swallow or mis-index it.)

## S7 — Test gap: no extend-after-extend (3-deep composition)

`extend_is_the_fork_composition` does ONE extend (1→2 segments). A real fork tree path is
deeper: root → mid → leaf built by `new(&[root]).extend(mid)?.extend(leaf)?`. The
`index - 1` arithmetic in `extend` (:110,128) and the "last segment is the stitch anchor"
logic are only exercised at depth 1. A 3-deep composition test would pin that `extend`
stitches against the *newly appended* last segment, not segment 0, and that the index in a
`BrokenStitch`/`MissingEndSnapshot` from a deep extend is correct.

**Suggestion:** add `extend_chains_three_deep` and a negative `extend_rejects_broken_deep_
stitch` asserting `BrokenStitch { index: 1 }` (not 0) when the third segment doesn't stitch
to the second.

---

## What cw2 still needs (assessment)

`determinism-hypervisor-cw2` is P0, OPEN, blocked on `3lt` (this bead, IN_PROGRESS). With
this `splice.rs` landed, what remains for cw2 to be unblockable:

1. **The `Clone`/`extend` ergonomics (I1)** — cw2 forks 1000 children off one prefix.
   Without `Clone`, the harness either rebuilds the prefix 1000× or can't reuse `extend`.
   This is the single most consumer-relevant item; resolve before cw2 starts.
2. **The harness glue itself** is NOT in this bead — cw2 must: boot → root snapshot → for
   each of 1000 children, seal a child segment, `Lineage::new(&[root, child])` (or
   `prefix.clone().extend(child)`), then for each `edge` call `verify_replay(slot, rail,
   cfg, SnapshotRef::from_bytes(edge.base_snapshot_id), counter, store, edge.log_bytes)`,
   threading the intermediate snapshot ref, and assert `VerifyDone` with matching
   `end_state_hash` and zero `Divergence`. `edges()` + `end_identity()` give it exactly the
   plan it needs; the `[u8;32]→SnapshotRef` hop (S2) is the only impedance, and it's one call.
3. **`Edge` carries `log: LogReader`, not raw bytes** — `verify_replay` takes `log_bytes:
   &[u8]`. `Edge` does NOT expose the underlying bytes (only the parsed `LogReader`, whose
   bytes are private). The harness will need the original `&[u8]` to feed `verify_replay`.
   It still has them (it owns the segment `Vec`s it passed to `new`), so this works — but
   note `Edge` is slightly mismatched to the consumer signature: it hands a `LogReader` the
   consumer must re-`parse` from bytes anyway (verify_replay.rs:52 re-parses). Consider
   `Edge` carrying `log_bytes: &'a [u8]` instead of (or alongside) the `LogReader`, since
   the consumer re-parses from bytes regardless. **This is the closest thing to a real
   consumer-fit gap after I1** — worth weighing as you wire cw2.
4. **Seed-distinctness audit (S3)** is a nice-to-have that would make cw2's "DISTINCT
   seeded burst" claim checkable pre-replay.

Net: `splice.rs` delivers the validated-path abstraction cw2 needs. The two items that will
actually bite the cw2 author are I1 (Clone) and item 3 (Edge exposes a re-parsed
`LogReader`, consumer wants bytes).
