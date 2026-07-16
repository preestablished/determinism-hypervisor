# Action items — iteration 80 (sr5), 2nd reviewer

Each item is self-contained (file, what, why).

### Critical

None.

### Important

- [ ] **Close the u8-fit blind spot in `every_proto_stop_reason_fits_the_u8_slot`.**
  **File:** `crates/dh-inputlog/tests/stop_reason_mirror.rs:13-31`.
  **What:** The `0..=255` scan plus `assert_eq!(seen, 8)` cannot observe a future proto variant
  numbered ≥256, and that is precisely the variant that would *not* fit the END u8 slot — so the
  test's stated guarantee ("every proto StopReason value must fit the u8 slot") is broader than what
  it checks. A 9th variant at 300 keeps the in-range count at 8 and `try_from(300)` succeeds, so the
  test stays green while the mirror claim becomes false.
  **Fix (pick one):**
  (a) Pin the *max known wire number* and that the first gap above it stays ≤ 255 — e.g. assert
  `StopReason::Faulted as i32 == 7`, `u8::try_from(7).is_ok()`, and `StopReason::try_from(8).is_err()`
  (first gap above the top). A ≥256 variant moves the first gap and breaks honestly; or
  (b) Keep the scan but add an explicit invariant assertion + comment: *"proto StopReason must never
  number any variant ≥256 — the DHILOG END slot is a single u8."*
  **Why:** This is the only test asserting the u8↔proto fit, the codec does not range-check the
  byte, and the count-pin overlaps the scan on the easy case while both miss the dangerous one.
  prost's `TryFrom<i32>` is closed (confirmed empirically, prost 0.13.5), which is what makes the
  ≥256 case decode-able-but-unseen.

### Suggestions

- [ ] **Document why these are free fns, not `From` impls.**
  **File:** `crates/dh-worker/src/proto_map.rs` module doc (lines 1-13).
  Add: free fns give the cast-ban grep a single named allowlist per crossing, and avoid tempting a
  blanket `From` that would push the non-total proto→domain direction into a lossy/panicking impl.
  Without this note a future contributor will refactor to `From` and lose the property.

- [ ] **Add the `as i32` cast-ban guard the bead's extended scope called for.**
  **File:** new lint-lane check (e.g. in `scripts/` or the clippy lane), referenced from
  `crates/dh-worker/src/proto_map.rs:5-9`.
  The module doc and bead sr5 NOTES both say to "grep-forbid 'as i32' casts on SlotState," but no
  such guard exists in `scripts/`, `.github/`, or `docs/`. Add `grep -rn 'SlotState as i32' crates/
  | grep -v proto_map.rs` (and the `StopReason` analog) to the lint lane so the discipline the doc
  promises is mechanically enforced before ol1 adds callers. **Note:** this is *extended* sr5 scope,
  not the core mirror pin — fine to defer to ol1, but it should be a tracked follow-up rather than
  left implicit.

- [ ] **Make the lying-casts assertion message name the recovery action.**
  **File:** `crates/dh-worker/src/proto_map.rs:74`. Change `"the order-divergence trap moved"` to
  something like `"SlotState↔proto cast divergence changed — a variant was added/renumbered; add
  the match arm and re-derive this count"`. The pin is good; the message should tell the next
  maintainer what to do.

- [ ] **Add a one-line note that proto→domain reverse conversions are deliberately deferred to ol1.**
  **File:** `crates/dh-worker/src/proto_map.rs` module doc. The reverse direction is *partial*
  (proto `*_UNSPECIFIED`, `NextSdkEvent`, `Faulted` have no domain producer yet) and belongs to the
  ol1 request path. Recording that keeps the domain-only asymmetry from reading as an oversight.

## Scope-discharge honesty (bead determinism-hypervisor-sr5)

- **Core ask DISCHARGED:** "cross-crate test that StopReason fits u8 and golden fixtures decode to
  intended variants" — done, with the I1 caveat above on the u8-fit half.
- **Extended scope (iter 79 review) PARTIALLY discharged:** the SlotState hand-written match +
  wire-number pin landed (the harder, more valuable half). The companion **`as i32` grep-ban is NOT
  implemented** (S2 / action item above) — the discipline exists only as prose.
- **Extended scope DEFERRED (correctly):** `runctl::StopReason::Faulted` is explicitly left for
  "when run control wires fault detection," matching the bead NOTES; the proto_map doc reflects this
  honestly. Out of scope for this iteration.
- Recommend: keep sr5 open (or spawn a follow-up bead) for the grep-ban, and confirm the I1 fix
  before the bead is closed, since the bead's title is specifically about the *u8 mirror coupling*.
