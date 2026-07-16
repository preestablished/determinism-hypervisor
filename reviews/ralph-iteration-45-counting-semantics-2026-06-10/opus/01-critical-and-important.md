# Critical and Important Findings

## Critical

**None.**

I attacked the headline claim hardest (an empirical assertion contradicting
the vendored spec) with a purpose-built isolation guest run live on the lab
box. The claim survived: each of the 3 region-exiting instructions retires
exactly 0 under `exclude_host=1`, REP MOVSB retires exactly 1, and the OUT
markers retire 0. The 997 arithmetic is the unique consistent explanation.

## Important

**None.**

The candidate Important issues I investigated and cleared:

### (cleared) Could a different decomposition also yield 997?
Alternative hypotheses — e.g. REP MOVSB retiring 0 plus one exiting
instruction retiring 1 — were ruled out DIRECTLY. The isolation windows
measured REP MOVSB = 1, CPUID = 0, MMIO read = 0, MMIO write = 0
*independently*, not as a sum. Only `1000 − 3` fits all six measured deltas.

### (cleared) Does any CODE depend on the wrong spec claim ("retires once")?
No. `boundary.rs::land_at` never assumes `+1`; it re-reads the counter after
every step and only declares a boundary at exact `c == target`, explicitly
treating mid-emulation exits as not-yet-retired. Because an exiting
instruction's RIP is skipped host-side with the counter unchanged, the engine
simply single-steps "through" it (one KVM_RUN, counter flat, loop continues)
— this is *already* the correct behavior for 0-retiring exits. `inject.rs`'s
`current.icount + 1` targets the next retirement boundary, which is correct
regardless of how many exits sit in between. `step_one_entry`'s
`debug_assert!(icount > 0)` only asserts forward progress, never a specific
delta. **Bead 0sc's "doc bug" disposition is correct; there is no latent code
bug.**

### (cleared) Mid-segment counter reset corrupting the delta?
`run_counting` resets the counter ONCE at boot. `run_segment`/`land_at` never
issue `PERF_EVENT_IOC_RESET` mid-segment — they only `arm_period` (which takes
effect from the current count) and toggle single-step. The S→E delta cannot
be corrupted by a reset.

### (cleared) PMI perturbing the count between S and E?
The smoke arms `NEVER_FIRES_PERIOD` (1<<62) and the whole guest runs ~1004
instructions — far under the budget — so the boundary engine takes the
single-step / direct-run path and never arms a firing period in this window.
Even if a PMI kick landed, INST_RETIRED is unaffected by the kick, and the
`EINTR` path in `land_at` only `clear_immediate_exit`s and re-reads the
counter (no double-count). Confirmed empirically: 6 live runs + a 20-boot
histogram all give exactly 997.

### (cleared) `jne .never` mis-jump silently passing/failing?
`.never` is `ret`, placed AFTER the E-OUT. If the never-taken branch were ever
taken, E-OUT would be skipped, `at_e` would stay `None`, and the test fails
loudly via `.ok_or("E marker never seen")`. No silent pass path exists. (And
`rax` is provably 0 at the `cmp`/`jne`, so it is never taken.)
