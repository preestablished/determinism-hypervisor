# Action items

Each item is self-contained (file path, exact change, rationale) so it can be actioned without
re-reading the review.

### Critical

_None._

### Important

- [ ] **I1 — Scope the §3.1 "MEASURED" claim to what was actually isolated; demote HLT and PIO-IN
      to "expected, not yet isolated."**
      File: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` lines 233–238.
      Current text names `(CPUID, HLT, PIO, MMIO)` and calls the whole set "MEASURED on the
      kvm-intel class." The counting guest only brackets CPUID, OUT, MMIO-read, MMIO-write
      (`COUNTING_EXIT_INSTRS_IN_REGION = 3` + the OUT markers). **HLT** parks in crt0 *after* the
      region (counting.asm line 122) and **PIO IN** appears nowhere in the region — bead gfb says
      "HLT retirement is NOT yet measured … measure it here before relying on it," and bead 0sc
      scopes the empiric to "PIO OUT, CPUID, MMIO access." IN exits are constrained-deterministic
      (hello.asm LSR-poll, boot_hello tests) but not isolated-to-zero. Reword so MEASURED covers
      `CPUID/OUT/MMIO` and HLT + IN are marked "same RIP-skip mechanism expected, not yet isolated;
      re-confirm via gfb." Suggested wording is in 01/I1.

- [ ] **I2 — Finish the stale-comment update in counting.asm: MMIO read/write still say "retire
      once."**
      File: `tests/nanokernel/asm/counting.asm` lines 21–24.
      The CPUID line (20) was updated to "retires ZERO (measured)" but the adjacent MMIO read
      (lines 21–22, "exits, retires once on the completing resume") and MMIO write (lines 23–24,
      "exits, retires once") still describe the refuted rule — and these are the very `XI`
      (exiting-macro) instructions at lines 76–77 that the new rule says retire zero. This
      contradicts line 13 and line 20 of the same file. Change both to "exits; retires ZERO
      (measured)". **Do NOT touch line 80** ("each retires exactly once") — that is correct; it
      describes plain branch instructions. Touching the file forces a nasm rebuild; confirmed safe
      (rebuild + `counting_smoke` pass).

### Suggestions

- [ ] **S1 — Soften §6.2 "run control subtracts its segment base internally."**
      File: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` lines 441–444. The device
      register being absolute vns is correct, but in code the base subtraction happens at the
      **caller boundary** before `TimerArm` is built (runctl.rs lines 106–113), not inside
      `timer_to_injection` (lines 124–137, no subtraction), and is a no-op today (`vns_base == 0`).
      Reword "internally" to "the run-control layer subtracts the segment's vns base when it
      converts to a segment-relative icount target (today the base is 0)," optionally citing
      `runctl::TimerArm` / `PvClock::armed`. Non-blocking; black-box description is defensible.

- [ ] **S2 — Quantify "bit-stable across cold boots" in §3.1.**
      File: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` line ~234. Bead 0sc and lib.rs
      lines 116–120 record "15+ cold boots bit-identical." Citing the count makes the claim
      auditable. Optional.

- [ ] **S3 — Move the §3.1 "(An earlier revision claimed …)" historiography out of normative
      prose.**
      File: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` lines 237–238. Style only — a
      CHANGELOG/footnote is a cleaner home than inline revision history in a living spec, to avoid
      accretion over future iterations. Keep the information somewhere; the placement is the
      suggestion. No correctness impact.
