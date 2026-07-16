# Positive Notes

### P1. Design thesis is faithful to the normative docs

The "validated sequence, not a byte-concatenated file" framing matches INTEGRATION §3 word
for word. The stitch rule `seg[i].end_snapshot_id == seg[i+1].base_snapshot_id`
(`splice.rs:88`) is exactly the induction edge in the §3 sequence diagram and API.md §3.4(3).
Resisting the temptation to build a flat v2 container in this repo (that's replay-renderer's)
keeps DHILOG v1 frozen, as required.

### P2. Correct, and correctly-defended, decision to NOT add a cross-segment watermark

API.md §3.4(3) is explicit: "Each log's icounts restart at 0 from its own base; there is no
global icount." The module enforces continuity purely through the content-addressed snapshot
stitch and per-segment validation — which is precisely the soundness argument ("equal ref ⇒
bit-identical state"). Inventing an `end_icount == next.base_icount` rule would have been
*wrong*; the module avoids it and documents why (lines 23-26).

### P3. Continuity case analysis is complete — no zero-to-zero stitch is reachable

The inner-`end_snapshot_id == [0;32]` check (`splice.rs:85`, and the `extend` analogue at
:111) runs **before** the `end == next.base` comparison. So a hostile lineage cannot stitch a
zero end ref to a zero base ref: any inner segment whose only "match" would be zero-to-zero is
rejected as `MissingEndSnapshot` first. A `[0;32]` base (BOOT segment) is implicitly root-only
and falls out of the stitch rule everywhere else. The ordering is load-bearing and correct.

### P4. Full validation is genuinely inherited from the reader

`Lineage::new` and `extend` both route every segment through `LogReader::parse`
(`splice.rs:71`, `:101`), which `reader.rs` confirms runs the complete battery: `SEALED`
required (`reader.rs:375`, rejects crash artifacts per §3.4.4), body_hash BLAKE3
(`reader.rs:278`), the (icount, seq) watermark and END identity (`validate_records`). So the
splice layer does not re-implement — or weaken — any per-segment guarantee.

### P5. Errors are loud, indexed, and forensically useful

Every `SpliceError` variant carries the offending segment `index`, and `Segment` wraps the
reader's own `ReadError` — so a failure points at exactly which segment and which check broke,
matching the project's "errors loudly" / divergence-tooling ethos. The corrupt-segment test
(`splice.rs:267-274`) asserts the index is propagated correctly through a body-hash flip.

### P6. Tests cover the real failure surface, not just the happy path

`continuity_violations_are_loud` exercises empty, broken stitch, dead-end inner segment,
config mismatch, clock mismatch, and a bit-flipped (corrupt) segment — each asserting the
*specific* error variant and index. `extend_is_the_fork_composition` checks both the compose
and the refuse-stranger paths. The edge-planning test verifies root-first order and the
leaf-only zero `end_snapshot_id`. This is the right shape for a security/correctness-critical
validator. All 3 pass; clippy is clean.
