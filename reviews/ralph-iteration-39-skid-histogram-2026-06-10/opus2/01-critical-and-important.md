# Critical & Important findings

## Critical

**None.** The gate is correct, the live test passes, the measurement is
deterministic, and the unsafe-free boundary is respected.

## Important

**None blocking.** The one item that rises above a pure suggestion is the
docs/empirics trail, recorded here because the prompt asked specifically that the
trail not contain a contradictory "exact skid" claim without context.

### I-1 (borderline; recommend doing it in this iteration) — annotate the iter-16 "exactly 18" note so the trail is not self-contradictory

`crates/dh-vmm/src/run.rs:19-21` still reads:

```
// Empirics (lab box, 40 runs, periods 100k/10k/1k/100): PMI overshoot is
// exactly 18 instructions, zero variance, period-independent — pure
// delivery latency, comfortably inside the 8192 skid margin.
```

This is **true and I reproduced it verbatim** (18, 6/6 runs) — but it is measured
on a real-mode `jmp $` self-spin, the trivial floor case. The new iteration-39
artifact reports 27..31 on the landing_loop guest. Anyone reading both will see
"18" and "27..31" as an apparent contradiction or a regression. They are neither:
the difference is instruction mix (the landing_loop's `imul`+dependent-chain+store
keeps more instructions in flight at NMI delivery).

**Recommendation:** add one clause to the run.rs note scoping it to the trivial
stream, e.g. "…on a real-mode single-instruction spin (the delivery-latency
floor); richer guests with stores and dependent chains skid ~10 more (see
`dh-cli skid`: 27..31 on the landing_loop) — still ≪ margin/2." This is a
2-line doc edit, not code, and it closes the only honesty gap in the trail.

Why borderline-Important rather than a Suggestion: the bead explicitly is about
the empirics trail, and shipping two un-cross-referenced "exact skid" numbers in
the same repo is exactly the kind of thing that mis-leads a future debugger into
chasing a phantom regression. Cheap to fix; high clarity payoff.

### Why nothing here is truly blocking

- The 27..31 vs 18 gap is **explained and benign** — both ≪ 4096; the gate (not the
  constant) is the deliverable. The gate has a 73× safety factor.
- The measurement loop is correct: counter is `reset()` once, reads are monotonic
  cumulative, `skid = after − armed_point` is self-correcting against inter-sample
  drift, and the `after < armed_point` guard catches stale signals. Bit-identical
  results across 5 runs confirm no overflow leakage between samples.
- The unsafe-free invariant is preserved exactly the right way: the `gettid`
  syscall is encapsulated in `dh_vmm::run::current_tid()` (with its own SAFETY
  note), and dh-cli keeps `#![forbid(unsafe_code)]`. The old
  `process::id() as i32` tid bug — which would have silently mis-routed PMI kicks
  from any worker thread — is removed, and grep confirms it exists nowhere else.
