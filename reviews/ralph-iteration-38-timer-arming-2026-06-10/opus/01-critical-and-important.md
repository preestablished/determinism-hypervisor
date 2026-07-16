# Critical and Important Findings

**None.** No correctness defects, no contract violations, no replay-identity hazards
were found. This file records the adversarial walks behind that conclusion so the
reasoning is auditable — each is a *resolved* concern, not an open issue.

---

## Walk A — ORDER CONTRACT: timer appended last is replay-stable (RESOLVED ✓)

`agenda.rs` ORDER CONTRACT (lines 51–55): `StopPoint::injections` stores indices into
`AgendaInputs::injections`, so replay identity requires both runs to present injections
in the same canonical (DHILOG) order. The new code (`runctl.rs` 78–86) builds:

```rust
let mut all_injections = seg.injections.to_vec();   // static, DHILOG order
let timer_slot = match seg.timer {
    Some(t) => { all_injections.push(timer_to_injection(t, clock, seg.start_icount)?);
                 Some(all_injections.len() - 1) }
    None => None,
};
```

The concern raised in the brief: in replay, static injections come from the DHILOG log
while the timer is re-derived from device state — is the merged order still stable?

**Resolved.** Three independent reasons:

1. **Same code both sides.** Record and replay run the identical `run_segment` body.
   `seg.injections` is presented in DHILOG order on both (caller's contract), and the
   timer is always pushed *after* them. So the merged vector's element order is
   construction-deterministic.
2. **Timer is a pure function of restored state.** `PvClock::armed()` returns
   `(timer_deadline_vns, vector)` from device fields that are snapshotted/restored
   (clock.rs `snapshot`/`restore`, `vns_base` restored via `set_vns_base`). The
   absolute deadline minus the segment's `vns_base` and the §4 ceil conversion are all
   pure, so the timer's icount is identical every run. No host-time / nondeterministic
   input feeds it.
3. **Delivery order is by icount, not by slice index.** `compile` orders points by
   `icount` (binary_search insert), and `point.injections` is ascending by index only
   as a *tie-break within one boundary*. The slice index is an identity tag, not the
   firing order. Even if the timer shares an icount with a static injection, the
   ascending-index ordering is deterministic (timer index is always the max, so it
   fires last at a shared boundary) and identical across runs.

Conclusion: append-at-end satisfies the ORDER CONTRACT deterministically. The only way
to break it would be to make `seg.injections` order or the timer derivation
run-dependent — neither happens here.

---

## Walk B — Clamp + silent agenda exclusion = correct one-shot semantics (RESOLVED ✓)

`timer_to_injection` (runctl.rs 33–46) clamps: `.max(start_icount + 1)`.
`agenda::compile` keeps an injection only if `at > start && at <= final_icount`
(agenda.rs 152). Trace the short/zero-budget case:

- Budget 0 → `final_icount == start` → clamped timer at `start+1` satisfies `> start`
  but fails `<= final_icount` → **silently excluded from the agenda**.
- Real (unclamped) deadline beyond budget → converted icount > `final_icount` → same
  silent exclusion.

In both cases `timer_fired` stays `None`, `injections_delivered` does not count it, and
**no error is raised**. Is silent non-fire correct?

**Yes — this is the intended one-shot contract.** `dh-devices/src/clock.rs` (88–99)
documents that the caller disarms *only when the timer fires*. A timer whose deadline
lies beyond this segment is not consumed: `disarm()` is never called for it, so the next
segment's `armed()` re-read returns the same deadline, re-derives a fresh
segment-relative icount against the new `vns_base`, and the timer fires in whichever
segment finally contains its deadline. This is exactly the "deadline beyond the segment
→ stays armed for the next segment" behavior the device contract promises. Silent
non-fire is *correct*; an error would be wrong.

The one gap is documentation, not behavior — see Suggestion S1.

---

## Walk C — delivered_icount is the queue boundary, not the entry RIP (RESOLVED ✓)

`TimerFired.delivered_icount = inj.delivered_icount` (runctl.rs 132). In the live test,
budget == deadline, so the injection point and the final-stop point merge into one
agenda boundary; `KVM_INTERRUPT(v)` queues the vector but the segment returns before the
next `KVM_RUN` entry, so the handler never runs in-segment. The AUX record therefore
reports "delivered at T" while the vector actually enters guest execution on the *next*
segment's first entry.

**Consistent with §3.4 and iter-34.** §3.4 step 3 defines delivery as: the vector "is
delivered on the next `KVM_RUN` entry, before any guest instruction retires", and step 4
fixes the recorded value as "the *actual* delivery boundary … `TIMER_FIRE.delivered_icount`".
`delivered_icount` is the **queue boundary** (the icount at which `KVM_INTERRUPT` was
issued and the next entry will deliver before any retirement) — not the RIP of the
handler. This is the same semantics iter-34 established for scheduled injections. Doc
alignment confirmed; no change needed.

---

## Walk D — finish threading on all four paths (VERIFIED ✓)

Every terminal path carries `timer_fired`:

- **Goal satisfied** — runctl.rs 318–326 ✓
- **Budget / HardCap** — 334–342 ✓
- **Paused** (roll-forward) — direct `SegmentOutcome { … timer_fired }` at 368–375 ✓
- **GuestHalted** — all four `finish_halted(seg, clock, delivered, timer_fired)` call
  sites (103, 112, 168, 359) ✓, and `finish_halted` forwards it into `finish` (436).

The merged-final-point path (delivery + final stop at one boundary, the live-test case)
sets `timer_fired` in the per-point injection loop (128–134) *before* the
`point.final_stop` check (329), so the budget-reached return correctly carries the fire.
Verified by the live test asserting `out.timer_fired` is `Some` with
`reason == BudgetReached`.

---

## Walk E — `timer_slot == Some(*idx)` indexes the merged vec (VERIFIED ✓)

`point.injections` holds indices into `injection_icounts`, which is built from
`all_injections` (runctl.rs 86). The injection loop reads `all_injections[*idx].vector`
(120) and compares `timer_slot == Some(*idx)` (128) — both index the *merged* vector, so
the slot tag is consistent. The `.expect("timer_slot implies timer")` (131) is sound:
`timer_slot` is `Some` only inside the `Some(t)` arm that also set `seg.timer`, and
`seg.timer` is immutable for the segment. No panic path reachable.
