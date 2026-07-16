# Action items

### Critical

None.

### Important

- [ ] **File a P1 bead for the `step_one_entry` disarm-parallel (I1).** The
  chained-injection single-step loop (`crates/dh-vmm/src/boundary.rs:224-263`,
  caller `runctl.rs:280`) has the same structural exposure 4a3 just fixed in
  `land_at`: it does not account for an emulator-MMIO-completion Debug exit,
  which it would mis-read as "entry complete" and return early from
  (under-stepping the entry → wrong `delivered_icount`/`delivered_rip`, a
  SILENT wrong-boundary rather than a loud overshoot). NOT reachable by any
  committed guest today — the chained `i>0` path needs interrupt delivery, and
  the only injecting guests (timer_guest, sti_window) record into plain guest
  RAM not MMIO, and pad_echo never enables its IRQ vector. It goes live the
  moment the M5/M6 device-bus run loop runs a guest that delivers interrupts
  AND touches MMIO in the same window (the timer_guest `arm` mode is waiting
  for exactly that, bead 40q's successor). Suggested:

  ```
  bd create "step_one_entry: emulator-MMIO Debug exit can under-step a chained-injection entry" \
    -d "boundary.rs:224 step_one_entry returns on the first VcpuExit::Debug. An emulator-delivered Debug (the singlestep-completion hook for an emulated MMIO instruction, the 4a3 mechanism) is indistinguishable from a real step-Debug here, so a chained-injection entry that single-steps onto an MMIO instruction would return early with a partial-entry boundary (wrong delivered_icount/rip) — a SILENT wrong-boundary. Unreachable today: the i>0 chained path (runctl.rs:280) needs interrupt delivery, and no committed guest delivers interrupts adjacent to MMIO (timer_guest/sti_window ISRs hit plain RAM; pad_echo has no IDT). Goes live with the M5/M6 device-bus run loop. Fix candidate: the same counter-based single-instruction backstop 4a3's notes propose for the MmioWrite-under-stepping case, OR distinguish completion-Debugs from step-Debugs. Add a doc note at boundary.rs:233 now." \
    -p 1 -l impl -t bug
  ```

- [ ] **(Optional, recommended this iteration) Add the I1 doc breadcrumb now.**
  Even if the fix is deferred, add a comment at `boundary.rs:233` documenting
  that an emulator-MMIO-completion Debug is indistinguishable from a genuine
  step-Debug here, that this is safe ONLY because no committed guest delivers
  interrupts adjacent to MMIO, and pointing at the new bead. This prevents the
  next agent from "completing" the device-bus run loop on top of a silent
  landmine.

### Suggestions

- [ ] **S1 — Fix the contradictory "19 retirements per iteration" comment** in
  `tests/nanokernel/src/lib.rs` (mmio_stepper_elf doc). 16 nops + sub + jnz =
  18 non-MMIO retiring instructions; "19" + "the three MMIO instructions never
  retire" cannot both be true. State the empirically observed count.

- [ ] **S2 — Reconcile the "every instruction offset in the body" coverage
  claim** (`boundary.rs:484-486`) with the true body length. Verified: the
  stride sequence covers all distances 1..23 (claim holds), but the LANDED
  OFFSETS cover all positions only if body = 19; if body = 18 the test reaches
  only 12/18 offsets. Either confirm body=19 (and fix S1) so the claim stands,
  weaken the comment, or switch to a stride coprime with the body for
  guaranteed exact-offset coverage.

- [ ] **S3 — Add a one-line "no const pin needed" comment** near
  `mmio_stepper_elf` in lib.rs, noting the probes are address-agnostic and
  never read MMIO_BASE/ITERS (so the elf_shape drift omission is intentional,
  not an oversight). No drift pin is actually required — this is the correct
  call as shipped.

- [ ] **S4 — Memorialize the corrected mechanism and the probe lesson.**
  `bd remember` (or a note on 4a3) that the prior "MMIO-write eats the trap"
  framing (iteration-50 comment, iteration-82 commit, the 4a3 description) is
  superseded: TF survives MMIO completions; the emulator's Debug delivery is
  what disarms. Add a breadcrumb in `boundary.rs` near the step-walk docs that
  raw-code probes must be long-mode guests (real mode can't reach the MMIO
  hole) — pointing at `mmio_stepper` as the working pattern.
