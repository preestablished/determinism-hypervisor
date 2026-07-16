# Critical & Important Findings

No **Critical** findings. The test compiles cleanly against the public API, the wiring is
correct, and every load-bearing assumption I checked against the implementation holds. The
items below are **Important** in the sense of "the claims should be scoped honestly before
this is the canonical M4-accept gate" — none are correctness bugs in the test, and none
block the merge.

---

## Important

### I1 — The "transparency" claim is unproven for the raw guest TSC; the divergence is real-but-invisible by construction

**File:** `crates/dh-worker/tests/m4_transparency.rs:1-8`, `:271-274` (module doc + the H1!=H2 failure message)
**Cross-refs:** `crates/dh-vmm/src/vcpu_state.rs:187-189`, `crates/dh-vmm/src/hash.rs:336-343`, `crates/dh-vmm/src/tsc.rs:65-79`

The two legs follow **physically different guest-TSC trajectories**:

- Control leg: the guest TSC is whatever cold boot set up, free-running off the host TSC
  across both segments. Nothing ever programs an offset.
- Roundtrip leg: `vcpu_state::restore` programs `KVM_VCPU_TSC_OFFSET` to
  `vns.wrapping_sub(host_tsc)` (`vcpu_state.rs:188-189`), so `guest_tsc == vns` at the
  restore instant and then free-runs from there.

The hash **deliberately masks** this: `canonical_vcpu_blob` writes the normalized `vns`
into the IA32_TSC slot, never the raw captured TSC (`hash.rs:336-343`). So a green H1==H2
proves nothing about TSC transparency. It happens to be *fine here* only because the
landing-loop guest never executes `RDTSC/RDTSCP/RDPMC/CPUID` (verified:
`tests/nanokernel/asm/landing_loop.asm` contains none), so the divergent TSC never reaches
guest RAM and never perturbs the full-RAM walk. A guest that *did* read the raw TSC into
RAM would expose the offset-vs-free-running difference in the page walk, and this gate
would still pass or fail on RAM, not on TSC fidelity.

This is consistent with the architecture's M2-deferral of TSC alignment, but the module
doc's opening ("any instruction-count drift, device-state leak, or RAM byte the restore
failed to reproduce shows here") and the failure message both read as an unqualified
transparency guarantee. They are accurate only under the implicit "for a guest that never
observes the raw TSC" caveat.

**Fix:** Add one sentence to the module doc making the caveat explicit, so a future reader
does not over-trust the gate.

```rust
//! SCOPE CAVEAT: this guest never reads the raw TSC (no RDTSC/CPUID), so the
//! control leg's free-running guest-TSC and the restored leg's vns-programmed
//! TSC_OFFSET are both invisible to the chain (hash.rs normalizes IA32_TSC to
//! vns). This gate therefore proves RAM/vCPU/instruction transparency, NOT raw
//! guest-TSC transparency — a TSC-reading guest is the M2 alignment bead's gate.
```

---

### I2 — `assert_eq!(r2.vns, c2.vns)` is tautological and gives false confidence

**File:** `crates/dh-worker/tests/m4_transparency.rs:270`
**Cross-refs:** `crates/dh-vmm/src/runctl.rs:174,312-314`

`vns` in a `SegmentOutcome` is computed as `seg.config.clock.vns_from_icount(point.icount)`
(`runctl.rs:312`), using `config.clock` — a fixed origin-0 `ClockRatio` — never the device
`PvClock` and never any restore-side `vns_base`. Both legs use the identical `config()` and
both land at the identical absolute icount (2e8, asserted one line up at `:269`). Therefore
`r2.vns == c2.vns` reduces to `f(2e8) == f(2e8)` for the same pure function `f` — it cannot
fail unless `:269` already failed. The same is true of the normalized-TSC slot in the blob.

This matters because the restore path's one genuinely interesting clock action —
`clk.set_vns_base(time.vns)` in `restore_engine.rs:300` — is **not observed by this test at
all**. `run_segment` never consults the device clock, and the landing-loop guest never
reads pv-clock MMIO, so a wrong `vns_base` on the restored bus would pass this gate
silently. The assertion's wording ("virtual time diverged") implies it is guarding restored
virtual-time continuity; it is not.

**Fix (smallest):** keep the assert but correct the comment so it is not mistaken for a
`vns_base` guard:

```rust
    // Both legs derive vns purely from config.clock.vns_from_icount(icount)
    // with identical config and identical icount, so this is a consistency
    // check on the icount landing, NOT a guard on the restored PvClock
    // vns_base (which run_segment never reads — see I2 / runctl.rs:312).
    assert_eq!(r2.vns, c2.vns, "vns derivation diverged");
```

**Fix (stronger, optional):** to actually gate restored `vns_base`, read it back —
`bus` is in scope after restore. Downcast the `PvClock` and assert
`clk.vns_base() == r1.vns` (mirrors `restore_engine.rs` test `:235-242`, which proves a
guest at segment-relative icount 0 reads the boundary's absolute vns). That turns a
tautology into a real check of the one device-state value the milestone restore mutates.

---

### I3 — Device/bus state is entirely outside the gate; the milestone message implies otherwise

**File:** `crates/dh-worker/tests/m4_transparency.rs:271-274`
**Cross-refs:** `crates/dh-vmm/src/runctl.rs:318-319,374-375,403-405` (every `push_final_link` passes `device_sections=&[]`)

Every hash link in both legs is built with empty device sections — `runctl` hard-codes
`&[]` at all three `push_final_link` call sites. So the chain comparison is blind to:

- the restored bus's `PvClock.vns_base` (1e8's vns) vs the control bus's unset base — the
  control leg in fact has **no bus at all**;
- pv-entropy state (`[9;32]` restored seed vs control's nonexistent entropy);
- any serial/pad device divergence.

The landing-loop guest touches none of these via MMIO, so the legs stay equal — but the
failure message's "device-state leak" clause claims this gate would *catch* a device leak.
It would not: device state is structurally excluded from the preimage here. `hash.rs`
*has* `device_sections()` for exactly this, and the path is dead in `runctl`.

This is arguably the single biggest gap in the "transparency" framing: the milestone is
"snapshot/restore is invisible," and the most snapshot-specific state (the device blob the
DHSNAP codec spends most of its lines on) is the part the gate cannot see.

**Fix (doc, minimum):** drop or qualify "device-state" in the failure message — replace
with "a vCPU-state or RAM leak," matching what the chain actually covers, and add a
one-line note that device-section transparency is not gated here (tracked as future work).

**Fix (test, optional but high-value):** since `bus` is live after restore and `take_snapshot`
captured the control-leg-equivalent device state, assert
`dh_vmm::hash::device_sections(&restored_bus)` equals the device sections of a freshly
`test_bus()`-then-`run`-equivalent bus, or at minimum assert the restored `PvClock.vns_base`
and `PvEntropy` regs match expectations. Even a single restored-`vns_base` assertion (I2's
stronger fix) closes most of this gap cheaply.
