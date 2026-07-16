# Critical & Important Findings

## CRITICAL

### C1. §3.1 overclaims HLT as "MEASURED" — bead `gfb` says it is explicitly NOT measured

`.agents/docs/determinism-hypervisor/ARCHITECTURE.md:233-235` (new text):

> VM-exiting instructions (`CPUID`, `HLT`, PIO, MMIO) retire **zero** guest instructions — MEASURED on the kvm-intel class (counting guest, bit-stable across cold boots/cores/processes/load; see `nanokernel::COUNTING_DELTA_AT_OUT_EXITS`).

The empirical basis (`COUNTING_DELTA_AT_OUT_EXITS = 997` = region(1000) − 3 in-region exiting instructions) measures **exactly three** constructs, enforced by the asm build:

- `tests/nanokernel/src/lib.rs:113-115` — `COUNTING_EXIT_INSTRS_IN_REGION = 3`: "CPUID, the pv-clock MMIO read, the serial-THR-mirror MMIO write."
- `tests/nanokernel/asm/counting.asm:114-116` — build fails unless `EXITCOUNT == 3` (CPUID + MMIO read + MMIO write).
- PIO OUT is exercised at the window edges (`S`/`E` markers) and contributes 0 to the window — that is also genuinely measured by `counting_smoke.rs`.

**HLT is not measured anywhere.** The counting guest's crt0 "parks in HLT" *after* the `E` marker OUT (`counting.asm:122`), i.e. outside the measured S→E window. In `runctl.rs:53,235-243` HLT is handled as a terminal STOP (`GuestHalted`), never bracketed for a retirement delta. Bead `gfb` records this verbatim:

> NOTE: HLT retirement is NOT yet measured (the smoke ends at HLT without bracketing it) — measure it here before relying on it.

So the new §3.1 sentence lumps HLT into the "MEASURED" set when only OUT/CPUID/MMIO were isolated. This is precisely the "a wrong spec sentence caused real implementation bugs" failure mode: a downstream implementer reading "HLT … retire zero … MEASURED" will treat HLT retirement as settled and skip the measurement bead `gfb` requires.

**Fix:** Scope the "MEASURED" claim to the constructs actually isolated. Suggested wording: list `CPUID`, PIO, MMIO as measured-zero; call out HLT separately as *expected* zero by the same exit-before-retirement mechanism but **not yet bracketed** (per bead gfb), to be confirmed by the M2 `counting_semantics` single-step attribution. Do not present HLT as measured.

---

## IMPORTANT

### I1. `counting.asm` is internally contradictory — MMIO lines still say "retires once"

The diff updated the file header (lines 11-16: "VM-exiting instructions retire ZERO") and the CPUID line (line 20: "retires ZERO (measured)"), but left the two MMIO lines stating the *old, refuted* rule:

- `tests/nanokernel/asm/counting.asm:21-22` — "MMIO read … exits, retires / once on the completing resume"
- `tests/nanokernel/asm/counting.asm:23-24` — "MMIO write … byte 'M', exits, retires once"

These directly contradict the same file's updated header and the new §3.1 (both say MMIO retires **zero**). A doc-reconciliation iteration whose entire purpose is to delete the "retires once on the completing resume" phrasing left two copies of that exact phrasing in the source-of-truth asm. (Grep for `retire.*once` / `completing resume` lands on these two lines.)

Line 80 ("branches … each retires exactly once") is **correct** and should stay — branches do not VM-exit.

**Fix:** Change `counting.asm:21-24` MMIO read/write annotations to "exits, retires ZERO (measured)" to match the header, CPUID line, and §3.1.

### I2. §6.2 "run control subtracts its segment base internally" does not match the merged code

`.agents/docs/determinism-hypervisor/ARCHITECTURE.md:441-444` (new text):

> The deadline is **ABSOLUTE guest vns** … never segment-relative — run control subtracts its segment base internally (mirrors §6.4's `at_frame` convention).

First clause is correct: the device register IS absolute guest vns (`clock.rs:88-98`, `armed()` returns `timer_deadline_vns`, which is absolute on the continuous `vns_base` axis). The **second clause is wrong about run control**:

- `runctl::timer_to_injection` (`crates/dh-vmm/src/runctl.rs:124-137`) calls `clock.icount_for_vns_target(timer.deadline_vns)` with **no base subtraction**; the only segment-aware step is `.max(start_icount + 1)` (the start clamp).
- `vt::icount_for_vns_target` (`crates/dh-vmm/src/vt.rs:51-55`) is pure origin-0: `ceil(target_vns * den / num)`. No base.
- `TimerArm.deadline_vns` is documented as **COUNTER-SPACE origin-0 vns** (`runctl.rs:106-113`), and that docstring assigns base subtraction to the **caller**, not run control: "when M4 restore gives segments a nonzero vns base, **the caller** subtracts the base from the device's absolute deadline BEFORE constructing this … the conversion itself is origin-0; only the start clamp is segment-aware."

So the two existing docstrings already disagree about who subtracts the base: `clock.rs:90-92` attributes it to run control (`icount_for_vns_target(deadline - vns_base_of_segment)`), while `runctl.rs:106-113` attributes it to the caller and states run control's conversion is origin-0. The merged code matches `runctl.rs` (origin-0, no subtraction; today `vns_base == 0` so absolute == counter-space and nothing is observably wrong). The new §6.2 sentence enshrines the `clock.rs` version — the one that does **not** describe the actual `timer_to_injection` mechanism.

No live bug (base is 0 everywhere; no production path yet reads `armed()` into a `TimerArm` — all `TimerArm` constructions are tests/gate with literal deadlines). But this is a normative doc making a false mechanism claim about a determinism-critical conversion, in an iteration whose premise is that wrong spec sentences cause bugs. When M4 wires `armed()` → `TimerArm`, an implementer following §6.2 would put the subtraction in run control; the code's contract (`runctl.rs:106-113`) requires the caller to have already done it — a double-subtract or no-subtract divergence risk.

**Fix:** Either (a) reword §6.2 to match the code's actual contract — "the deadline is absolute guest vns; the segment is responsible for rebasing it to the counter-space (origin-0) target before run control's origin-0 `icount_for_vns_target` conversion (today the base is 0)" — or (b) if the intended design really is for run control to subtract the base, that is a code change out of scope for a doc iteration; file it. Also reconcile the contradicting `clock.rs:90-92` vs `runctl.rs:106-113` docstrings while you are here. The "mirrors §6.4 at_frame" analogy is fine in spirit (both are absolute device values mapped to segment-relative icount via a per-segment table), but §6.4 says the *frame table* maps F→icount, not that run control subtracts a base — so the analogy slightly misdescribes both surfaces.

### I3. IMPLEMENTATION-PLAN.md M2 accept still states the now-false "counter delta exactly 1,000"

`.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md:44-47`:

> **Accept:** `counting_semantics` test: single-step a known 1,000-instruction nanokernel sequence (including REP MOVS, CPUID, MMIO exits); **counter delta exactly 1,000**; REP retires as 1.

Per the merged §3.1, a single-step counter delta over a 1,000-instruction region that *includes* CPUID + 2 MMIO exits is **997**, not 1,000 (the 3 exiting instructions retire zero). `counting_smoke.rs:158-164` already asserts exactly `COUNTING_DELTA_AT_OUT_EXITS` (997). This M2 acceptance line is now a known-false normative claim — the same `retire-once`-equivalent arithmetic this iteration set out to eliminate, surviving one section over.

Bead `gfb` already carries the corrected single-step attribution in its notes (REP +1, CPUID/MMIO/OUT +0, plain +1), so the implementer will not be misled — but the vendored doc is wrong. This file is outside the iteration's stated diff, so it is Important rather than Critical/blocking; but a §3.1 reconciliation that leaves §M2's "exactly 1,000" standing is incomplete.

**Fix:** Update the M2 accept criterion to the measured attribution: single-step delta = 1,000 − (count of in-region VM-exiting instructions), with REP retiring as 1 and CPUID/MMIO/PIO retiring 0 — i.e. 997 for the current guest. (Or scope the iteration to acknowledge it as deliberately deferred and bead it.)
