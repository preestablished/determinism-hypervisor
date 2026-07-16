# Critical & Important findings

**None.**

I went looking adversarially for a way to break the landing and could not find one. This file
documents the failure modes I probed and why each is safe, so a later reviewer does not have to
re-derive them.

## Adversarial cases tried — all SAFE

### 1. `land_at(target == current counter)` does NOT run the guest — VERIFIED LIVE
Walk: the loop reads `c` first; `c == target` matches on the first iteration before any
`arm_period`/`run`, takes `get_regs`, and breaks `Ok`. I verified this live (scratch): landing
twice at the same target returned immediately the second time with identical `rip` and `rcx` and the
counter unmoved. No accidental over-run. (`boundary.rs:112-124`.)

### 2. 50 random ascending targets on one guest — exact each time, zero drift — VERIFIED LIVE
The real fear with a compose-across-calls engine is cumulative error: a half-step or off-by-one that
only shows after dozens of landings. I ran 50 ascending pseudo-random targets (mix of far PMI+step
and near pure-step) on a single guest, asserting `b.icount == target` AND `counter.read() == target`
each time. All exact; repeated 4x with zero variance. (Bead 8g1 is the 10,000-target version; this
50-target smoke would have caught systematic drift now and did not.)

### 3. Overshoot is loud, never absorbed — VERIFIED LIVE
`stale_target_is_a_loud_overshoot_live` confirms a target already in the past surfaces as
`Overshoot { target, counted }`, fatal, with no silent absorption (risk R1). `c > target` is checked
*first*, before the `c == target` equality, so a past target can never masquerade as "already there."

### 4. Spurious / stale EINTR in stepping mode does not corrupt the landing
Per §3.1, a queued RT kick can EINTR a later, legitimate `KVM_RUN` with the counter short of target.
Both the far and near arms treat EINTR as a stop *request*: `clear_immediate_exit(&mut guard)` then
loop and re-read the counter — never assume the period was reached. The counter re-read is the only
progress signal, so a stale kick costs at most one wasted iteration. (`boundary.rs:133-137, 156-158`.)
`clear_immediate_exit(&mut guard)` compiles because `KickGuard: DerefMut<Target = VcpuFd>` coerces
`&mut guard` to `&mut VcpuFd` — confirmed by a clean build.

### 5. Single-step TF never miscounts host trap handling
The counter is opened `exclude_host | exclude_hv | exclude_idle` (`counter.rs:60-62`), so the
KVM_EXIT_DEBUG trap delivery and the host-side loop iteration are invisible to the count. Only
guest-mode retirements move it; a debug trap with RIP unchanged (mid-REP) does not. The §3.2 REP rule
is therefore enforced purely by "trust the counter, not +1."

### 6. PMI period cannot overflow / throttle during the stepped tail
Before turning on single-step the engine arms `NEVER_FIRES_PERIOD` (`1<<62`, `counter.rs:27`),
which cannot be reached inside a segment (hard cap 1e10 instructions), so the tight step loop never
re-arms a small period and never trips `perf_event_max_sample_rate` (the iteration-16 hazard). The
counter stays *enabled* through stepping as §3.2 requires. One arm per far approach, one park per
near approach — O(1) arms per landing, as the module header claims and the tests exercise.

### 7. Single-step is dropped on EVERY exit path
`if stepping { set_singlestep(&mut guard, false)?; }` runs after the loop regardless of whether
`result` is `Ok` or `Err`, so no caller ever inherits a vCPU left in `KVM_GUESTDBG_SINGLESTEP`
(risk R10). (One nuance about this `?` is a *suggestion*, see 02.) On the never-stepped (pure far,
or immediate `c==target`) path, `stepping` stays false and `set_guest_debug` is correctly never
called — control=0 disable is not needed because enable was never issued.

### 8. vPMU disabled (iter-30) removes guest perturbation of the host counter — POSITIVE
Because the guest has no vPMU, it cannot issue its own perf/PMC operations that would perturb the
host `INST_RETIRED` counter. This is load-bearing for determinism and is worth keeping in mind as a
dependency (noted positively in 03).

## Why no Important either

The two things that could plausibly be "Important" — (a) the post-loop `?` shadowing an Ok boundary,
and (b) the absence of a defensive single-step reset at entry — are both *conservative* current
behavior, not bugs, and are filed as suggestions in 02 with rationale.
