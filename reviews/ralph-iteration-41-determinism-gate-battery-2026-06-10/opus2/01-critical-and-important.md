# Critical & Important

## Critical

None.

---

## Important

### I-1 — `TimerArm::deadline_vns` doc says "segment-relative"; every caller treats it as absolute counter-space vns (base 0)

**Files:**
- `crates/dh-vmm/src/runctl.rs:106-114` (the doc comment + struct)
- `crates/dh-vmm/src/runctl.rs:120-133` (`timer_to_injection`)
- `crates/dh-vmm/src/vt.rs:51-55` (`icount_for_vns_target`)
- `tests/determinism/tests/timer_determinism.rs:26-32` (the caller that exposes it)

**The contradiction.** The doc comment on `TimerArm` states:

> `deadline_vns` is segment-relative here (the caller subtracts the segment's
> vns base from the device's absolute deadline).

But the conversion does **not** treat it as segment-relative. `timer_to_injection`
calls `clock.icount_for_vns_target(timer.deadline_vns)` which computes
`ceil(deadline_vns * den / num)` — an **absolute icount measured from
vns/icount origin 0**, not from the segment start. It then clamps with
`.max(start_icount + 1)`. There is no subtraction of any segment vns base
anywhere in the path; the only segment-relative operation is the clamp, which
is a floor, not a rebasing.

**Why it nonetheless passes today.** The `timer_determinism` test arms
`deadline_vns = (k+1)*1_000_000` for segment `k`, whose `start_icount =
k*1_000_000`, and the counter is **never reset across segments** — it is one
continuous counter space for the whole boot, with vns base 0 and a 1:1 clock.
So:

- Segment 0: `start=0`, `deadline_vns=1M` → `icount_for_vns_target(1M)=1M`,
  `max(0+1)=1M`. Lands at absolute 1M. Budget is also 1M, so the agenda merges
  the injection point and the final stop: the vector is queued, the segment
  finishes. (This is the source of the FIRES-1 ISR count — see S-1.)
- Segment 1: `start=1M` (the queued vector has not delivered, so the counter is
  still 1M), `deadline_vns=2M` → `icount_for_vns_target(2M)=2M`,
  `max(1M+1)=2M`. Lands at absolute 2M.

Because the vns base is 0 for the whole boot, "absolute vns" and
"segment-relative vns" produce the same icount and the arithmetic is internally
consistent. The test is **using base-0 as an identity**, not honoring the
documented "subtract the segment's vns base" contract — there is nothing to
subtract.

**Adjudication.** The test is *not* using the API as the doc literally
describes; it is exploiting base-0. The code is correct; the **doc is the
defect**. In this iteration's usage `deadline_vns` is unambiguously
**counter-space-absolute vns** (origin 0), and `timer_to_injection` is an
absolute conversion with a start-clamp.

**Why it matters (the time bomb).** The moment M4 restore introduces a nonzero
segment vns base — i.e., a segment that resumes at icount/vns ≠ 0 — the doc's
"caller subtracts the vns base" instruction and the code's absolute conversion
**diverge**. A caller who follows the doc (subtracts the base, passing a
*relative* deadline) will have `icount_for_vns_target` treat that relative
number as absolute and land the timer at the wrong icount — silently, since the
clamp only catches deadlines that fall before `start+1`. This is exactly the
class of bug the determinism gate exists to prevent, hidden in the one place
the gate's own arithmetic can't see (the gate fixes the boot at base 0).

**Recommended fix (doc + a guard, no behavior change now):**
1. Rewrite the `TimerArm` doc to state the actual contract: `deadline_vns` is
   the **absolute pv-clock deadline in counter-space vns (origin 0)**;
   `timer_to_injection` converts it to an absolute agenda icount and clamps to
   `start_icount+1`. Drop the "segment-relative / caller subtracts the vns
   base" sentence — it describes a rebasing the code does not perform.
2. Update the `icount_for_vns_target` / `timer_to_injection` doc on
   `runctl.rs:116-119` to match (it already says "smallest icount whose vns
   reaches the deadline", which is the absolute reading — just make the origin
   explicit).
3. Add a one-line note on the M4 restore work (bead) that segment vns bases
   must be folded into `deadline_vns` by the *caller producing an absolute
   number*, OR the API must grow an explicit `segment_vns_base` parameter so
   the conversion rebases internally. Decide this before M4 freezes restore.

This is "Important" rather than "Critical" only because nothing is wrong on
this branch — the gate is honest about what it tests today. It is the
highest-leverage thing to fix because it is a documented contract that the
first nonzero-base caller will trust and get burned by.
