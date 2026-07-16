# Action Items — Iteration 47 Doc Reconciliation

### Critical
- [ ] **C1 — Stop presenting HLT as MEASURED in §3.1.** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md:233-235`: the "MEASURED" set is CPUID + PIO + MMIO (the build-enforced 3 in-region exits plus the OUT markers). HLT parks *after* the measured S→E window (`counting.asm:122`) and is a terminal STOP in `runctl.rs:53,235-243` — never bracketed. Bead `gfb` explicitly: "HLT retirement is NOT yet measured … measure it here before relying on it." Reword §3.1 to mark CPUID/PIO/MMIO as measured-zero and HLT as *expected* zero but **not yet measured (per bead gfb / the M2 counting_semantics attribution)**.

### Important
- [ ] **I1 — Fix the self-contradiction in `counting.asm`.** `tests/nanokernel/asm/counting.asm:21-24`: the MMIO read/write annotations still say "exits, retires / once on the completing resume" and "exits, retires once," contradicting the file's own updated header (lines 11-16) and the updated CPUID line (line 20). Change both to "exits, retires ZERO (measured)." Leave line 80 ("branches each retires exactly once") — branches do not VM-exit, so it is correct.
- [ ] **I2 — Correct §6.2's base-subtraction mechanism.** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md:441-444`: "run control subtracts its segment base internally" is false against the code — `runctl::timer_to_injection` (`runctl.rs:124-137`) and `vt::icount_for_vns_target` (`vt.rs:51-55`) are origin-0 with no base subtraction; the base subtraction is the **caller's** future (M4) job per `runctl.rs:106-113`. Reword to: deadline is absolute guest vns, the segment rebases it to counter-space (origin-0) before run control's origin-0 conversion (base is 0 today). Keep the "absolute, never segment-relative" first clause (correct). If the design intent really is run-control-side subtraction, that's a code change — file a bead, don't assert it in the doc.
- [ ] **I2b — Reconcile the two contradicting timer docstrings** while fixing §6.2: `crates/dh-devices/src/clock.rs:90-92` (says run control subtracts) vs `crates/dh-vmm/src/runctl.rs:106-113` (says caller subtracts, conversion origin-0). Make both match the code.
- [ ] **I3 — Fix the now-false M2 acceptance.** `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md:44-47`: "counter delta exactly 1,000" over a region including CPUID/MMIO exits contradicts the merged §3.1 (it is 997; `counting_smoke.rs` asserts 997). Update to "delta = 1,000 − in-region VM-exiting instruction count; REP retires as 1; CPUID/MMIO/PIO retire 0." Out of the stated diff scope, so either fold it in or bead it explicitly.

### Suggestions
- [ ] **S1** — Align `device_exercise.asm:171-175` "(doc contradiction tracked in beads)" tail with `lib.rs`'s "vendored fixed; upstream tracked" phrasing.
- [ ] **S2** — Add a one-clause §3.1 caveat that the PIO measurement assumed single-byte OUT (not REP-string PIO); see `counting_smoke.rs:88-92` aliasing warning.
- [ ] **S3** — Reword `ARCHITECTURE.md:265` "(count unchanged until retirement)" to drop the implied eventual retirement, matching the §3.1 zero rule.
- [ ] **S4** — File a cleanup bead for the latent M4 timer-base ownership footgun (independent of I2's doc fix).
- [ ] **S5** — Soften §3.1 "never retiring" to "contributes zero to the retirement count on both sides of the exit" to avoid implying the instruction's effects don't occur.

### Verification (already run, all green — informational)
- [x] `cargo test --workspace` — all pass incl. `counting_smoke` (997, bit-stable ×2), `channel_interop`, `timer_determinism`, `if0_deferral`.
- [x] `cargo clippy --workspace --all-targets` — clean.
- [x] Working tree clean.
