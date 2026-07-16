# Positive Notes

### P1. Assembly-time exactness is the right design
`counting.asm:38-42, 95-103`. Enforcing the 1,000-instruction region with an
`%assign` counter and a `%if ICOUNT != 1000 %error` makes the count a BUILD
invariant, not a runtime hope. I reproduced the arithmetic independently
(18 I-macro instrs + 2×4 loop = 26, + 974 PAD = 1000) — it is exactly right,
including the manual `+2*LOOP_ITERS` for the dec/jnz body emitted outside the
`I` macro. This is a genuinely robust way to ship a "known N" guest.

### P2. The empirical claim is CORRECT and well-evidenced
The headline assertion — VM-exiting instructions retire 0 under
`exclude_host=1`, contradicting §3.1 — is true. My isolation experiment
confirmed each construct independently (CPUID=0, MMIO r=0, MMIO w=0, REP=1,
OUT=0). Filing bead 0sc rather than silently "fixing" the count to match the
spec is exactly the right call: the measurement is ground truth, the doc is
stale.

### P3. The doc comment on `COUNTING_DELTA_AT_OUT_EXITS` is exemplary
`lib.rs:114-131`. It states the measurement provenance (lab box, 15 cold boots
bit-identical), the mechanism (exit-before-retire + host RIP skip), the spec
contradiction, the determinism-is-unaffected reassurance, AND the arithmetic.
This is how an empirically-derived constant should be documented.

### P4. Determinism actually holds, live
counting_smoke passed 6/6 of my runs; the prior reviewer scratch showed a
20-boot histogram collapsing to `{997: 20}`. The product invariant
(bit-identical replay) is real here, not asserted.

### P5. The boundary engine already handles 0-retiring exits correctly
`boundary.rs:118-172`. Because `land_at` re-reads the counter after every step
and only declares a boundary at exact equality (never `+1`), an instruction
that exits and retires 0 is transparently stepped over. The empirical
discovery does NOT require any boundary-engine change — the engine was written
to "never assume +1," which turns out to be exactly what this class needs.

### P6. The not-taken branch fails loudly, never silently
`.never` = `ret` placed after the E-OUT means a mistaken jump skips E and trips
`at_e.ok_or("E marker never seen")`. No silent-pass hazard.

### P7. Cross-arch hygiene maintained
The new test is `#![cfg(target_arch = "x86_64")]`-gated; aarch64 clippy of the
whole workspace is clean. The KVM-only smoke compiles to empty off-x86 as
intended.
