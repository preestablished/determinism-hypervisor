# Suggestions (non-blocking)

### S1. Reuse `common::Rig` / `common::kvm_usable` instead of a 5th boilerplate copy

`landing_precision.rs` reimplements `kvm_usable()` and the entire
boot/route/arm/reset/enable sequence in `land_sequence`. That sequence is
byte-for-byte the body of `common::Rig::boot` (tests/determinism/tests/
common/mod.rs), and `kvm_usable` is identical to `common::kvm_usable`.
`if0_deferral.rs` and `timer_determinism.rs` already `mod common;` and
call `common::kvm_usable()`; `counting_smoke`, `m1_acceptance`,
`regression`, and now `landing_precision` each carry their own copy — so
the duplication is now 4 copies of a function that already exists shared.

The honest blocker the author likely hit: `Rig` only exposes
`run_one` (segment/`run_segment`-based), but this test needs raw
`land_at`. That's solvable without a new boilerplate copy because
`Rig.slot` and `Rig.counter` are `pub`:

```rust
mod common;
use common::{kvm_usable, Rig};
// ...
let mut rig = Rig::boot(elf, cmdline)?;
for &t in targets {
    let b = land_at(&mut rig.slot.vcpu, &rig.counter, t, &margins_for(i), &mut on_exit)?;
}
```

If `Rig::boot` building a `MachineConfig`/`StateHashChain` it doesn't need
feels heavy, the cleaner refactor is to add a thin
`Rig::land(&mut self, target, margins, on_exit)` next to `run_one`, so the
landing path and the segment path share one rig. Either removes ~30 lines
of exact-duplicate plumbing and one more `kvm_usable` clone. Suggestion
only — consistent with siblings as written, but `common` exists precisely
to stop this.

### S2. Lock the backward-landing contract once, here (cheap)

All targets are sorted ascending, so every landing moves forward; the
backward-direction path (a target below the current count must be a loud
`Overshoot`, never absorbed) is never exercised by *this* file. It **is**
covered by `boundary.rs::stale_target_is_a_loud_overshoot_live`, so this
is not a gap in the suite — but a one-call negative assertion at the end
of the landing test (land at N, then deliberately land at N-1, expect
`BoundaryError::Overshoot`) would keep the regression-direction contract
visible right where the forward contract is asserted, at ~0 cost. Pure
defense-in-depth; skip if you prefer to keep the assertion in one place.

### S3. PRODUCTION_PREFIX = 100 vs runtime

The 100-target production-margin (8192) prefix is ~8192 single-steps each
and dominates the runtime (the run I timed was ~95 s total, vs the ~71 s
the bead estimates — see S6). The prefix's *job* is to prove
margin-independence on real targets at the production margin; 20 targets
(~6 s) would prove the same property with far less wall-clock, while 100
buys broader coverage of the prefix's residue/skid-tail distribution.
This is a judgment call, not a defect — flagging the cost/coverage knob so
it's a deliberate choice. If CI wall-clock matters, consider 20–30.

### S4. Tiny doc precision: "RCX is a mid-REP detector" holds only at landed boundaries

The asm header and the lib.rs doc say RCX is 64 at the REP start and 0
"everywhere else." That's true at *landed instruction boundaries* (which
is all the test ever observes), but strictly RCX takes 64..1 *during* the
REP's internal iterations — those just never appear as a landed boundary
because mid-REP traps don't retire. The committed comment in
`land_at` (lines 9-12, 123-126) already explains exactly this. A half-line
in rep_loop.asm ("at any landed boundary") would make the standalone asm
self-consistent with the engine's REP rule. Cosmetic.

### S5. `rep_starts > REP_TARGETS / 20` floor — document the 1/6 expectation

The coverage floor asserts `> 50` REP-start landings out of 1,000. The
true expectation is ~1/6 (one of six residue classes is the REP start), so
~167 — my scratch run saw the rcx==64 class populated as expected. The
`/20` floor (50) is a deliberately loose lower bound, which is fine, but a
one-line comment ("expect ~1/6 ≈ 167; floor is intentionally slack") would
stop a future reader from thinking 50 is the modeled value.

### S6. Runtime note vs the bead estimate

Bead 8g1 estimates ~71 s; the full `landing_precision` run on this lab box
measured ~95 s (both tests, single-threaded). Not a problem, but if the
kvm-intel lane has a per-test timeout budget, calibrate it against the
observed ~95 s, not the estimate.
