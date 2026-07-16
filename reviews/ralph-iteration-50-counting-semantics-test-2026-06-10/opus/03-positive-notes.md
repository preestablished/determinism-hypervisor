# Positive notes

- **The engine fix is minimal and correct.** Two `set_singlestep(&mut guard, true)?` calls,
  placed exactly on the handled-exit paths, with comments that state the measured mechanism
  (emulator completes the instruction and clears TF without delivering #DB). It re-arms TF
  where the trap was eaten and is a harmless re-assertion where it survived. The error-path
  single-step teardown (R10 invariant) is untouched and still runs on every exit.

- **The regression test is a real guard, not a tautology.** Reverting the boundary.rs change
  makes `landing_across_an_mmio_write_does_not_free_run` fail loudly with
  `Overshoot { target: 20, counted: 1003 }`. This is the gold standard for a fix-plus-test
  pairing: the test demonstrably bites when the fix is absent.

- **The trap-eating premise is reproducible from first principles.** A raw single-step probe
  (no engine code) shows the exact asymmetry the iteration claims: MMIO writes eat the trap,
  MMIO reads and PIO OUTs keep it. The iteration did not just assert this — it is measurable
  by anyone on this class.

- **The attribution test is genuinely per-instruction.** Every §3.1 case is isolated:
  996 plain (+1), REP MOVSB (255 frozen-RIP traps +0, exactly one +1 advance), CPUID
  (no-exit/advance/+0 — a unique signature), MMIO read +0, MMIO write +0, S→E window == 997.
  The sums are cross-checked (`retired == 997`) and the whole trace replays bit-identically
  from a second cold boot. The `panic!("unclassifiable step ...")` arm is a good drift alarm.

- **The HLT empiric closes a real gap cleanly.** Measuring park-loop hlt/jmp cycles WITHOUT
  single-step (plain `vcpu.run()` → Hlt) and asserting each delta == 1 is a sound decomposition:
  jmp is non-exiting so it must retire (+1), hlt exits so it retires 0; delta 1 = jmp(1)+hlt(0)
  is the only consistent attribution. Discarding the first (i==0) partial cycle is handled
  correctly. This legitimately promotes HLT from "expected" to "measured" in §3.1.

- **The doc update is honest about scope.** §3.1 moves HLT into the measured-zero set with the
  attribution evidence, and explicitly keeps PIO IN as EXPECTED-but-not-isolated (constrained
  only by IN-heavy boot icount stability). It does not overclaim. The §3.2 pseudocode now
  carries the re-arm rule with the mechanism inline.

- **The single-step vs free-running cross-check holds.** counting_smoke measures the S→E
  window as 997 free-running; counting_semantics measures the same 997 under single-step —
  proving stepping does not change retirement, which is the load-bearing assumption of the
  whole near-approach engine.

- **No blast radius.** The re-arm did not perturb any previously-passing path; 209/209
  workspace tests pass, both arches clippy-clean.
