# Action items

### Critical

(none)

### Important

- [ ] [bead `gfb` description; `tests/nanokernel/src/lib.rs:115-145`] Widen
      bead `0sc`'s scope (or file a child bead) to also correct bead `gfb`'s
      acceptance criteria. `gfb` (P0, the M2 single-step acceptance) still
      says "counter delta exactly 1,000; CPUID/HLT/MMIO-exiting instructions
      retire exactly once on the completing resume" — the exact claim d34
      measured false. Change it to "delta ==
      nanokernel::COUNTING_DELTA_AT_OUT_EXITS (997 on the kvm-intel class);
      VM-exiting instructions retire zero." Separately, mark **HLT**
      retirement as still-unvalidated on this class (the smoke never measures
      it — the window is `s=6 → e=1003`, HLT is the terminal park outside the
      window), rather than asserting it retires once.

### Suggestions

- [ ] [`tests/determinism/tests/counting_smoke.rs:88-92`] Add a one-line
      comment that the 997 count is a retirement-semantics property
      independent of the pv-clock read value, so the M2 test (gfb) can reuse
      the figure even though it drives the real device bus; note that gfb is
      where the real-bus path (clock.rs REG_VNS returns nonzero monotone vns)
      gets validated, not here.
- [ ] [`tests/determinism/tests/counting_smoke.rs:71-77`] Comment that the
      marker loop assumes single-byte OUTs; a future batched-marker edit
      (e.g. rep outsb) would alias `at_s`/`at_e` onto one counter read and
      silently yield delta 0.
- [ ] [`tests/nanokernel/asm/counting.asm:75-78`] Comment that `.never`
      doubles as the post-E-OUT fall-through `ret` target, so no instruction
      is ever inserted between the E-OUT and the ret.
- [ ] [`tests/nanokernel/src/lib.rs` / `counting_smoke.rs:84`] Introduce a
      `COUNTING_MMIO_THR_GPA = 0xD000_6008` constant in lib.rs and have the
      smoke consume it, removing the hardcoded literal so a future MMIO_BASE
      change can't desync the test filter (mirrors the existing bootinfo.inc
      drift-test discipline).
