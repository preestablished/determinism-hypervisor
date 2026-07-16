# Critical and Important Findings

## Critical

**None.**

The two riskiest claims this acceptance makes were interrogated by execution and both held:

1. **Margin-independence (§3.2)** — confirmed under a *stricter* test than the suite applies (see below).
2. **No mid-REP boundary** — confirmed; the RCX detector is sound and the at-entry false-failure hazard was probed and found absent on this box.

## Important

**None that block merge.**

The points below were investigated as candidate Important issues and downgraded to Suggestions / Positive notes after running the evidence. They are recorded here so the next reviewer sees the reasoning, not just the conclusion.

### (Investigated → not a defect) The wide-spread margin evidence is confined to the prefix

Re-derivation of what the shipped cross-boot `assert_eq!(first, second)` actually compares, per index:

- **Indices 0..99** (the 100 *smallest* targets, since `targets` is sorted ascending and the prefix is `i < 100`): boot A margin **8192/1024** vs boot B margin **128/128**. This is a genuine wide-spread (64x) margin-independence comparison — but only on the smallest ~[1000, ~1M) targets.
- **Indices 100..9999**: boot A margin **256/256** vs boot B margin **128/128**. Tight-vs-tight (2x). This still proves cross-boot determinism + cross-margin equality, but it is *weaker* evidence of independence than the prefix.

So the wide-spread independence evidence in the shipped test lives only on the 100 smallest targets. This is acceptable for acceptance purposes (independence is a spec guarantee, and the prefix exercises it on real targets across a 64x spread), but it is not as decisive as it could be on large targets.

**I closed this gap by experiment.** A scratch run landed the SAME 20 targets — `1000, 1001, 1002, 5000, 12345, 200_000, 500_000, 1_000_000, 1_000_001, 2_000_000, 3_333_333, 5_000_000, 7_777_777, 10_000_000, 20_000_000, 40_000_000, 60_000_000, 80_000_000, 90_000_000, 98_999_999` — at margins {8192/1024, 4096/512, 64/64} across three cold boots. All three produced bit-identical `(icount, rip, rcx)` tuples, including the near-99M targets. **Margin-independence holds across a 128x margin spread over the full target range.** This is strictly stronger than the shipped test and removes any doubt. (Scratch deleted; tree clean.) See a sharper-spot-check suggestion in `02`.

### (Investigated → not a defect) RCX-at-entry could be garbage on the first iteration

`crates/dh-vmm/src/boot.rs:239-243` does `regs = vcpu.get_regs()` then sets only `rip`, `rsi`, `rflags` — RCX is left at whatever KVM's vcpu carries. The test asserts `b.rcx == 64 || b.rcx == 0` on *every* landing, so a garbage RCX before the first `mov rcx,64` would be a false §3.2 failure *if* a target landed there.

Probed it: landing at icount 1..6 of `rep_loop` shows **RCX = 0** (KVM zeroes GPRs at vcpu entry on this box), RCX = 64 at icount=7 (right after `mov rcx,64`). So:

- RCX is 0, not garbage, before the controlled `mov rcx,64` → even an icount<7 landing would *pass* the `rcx ∈ {64,0}` assert.
- The `TARGET_FLOOR = 1000` (≈166 iterations into a 6-instr loop) independently guarantees no target lands in the first iteration anyway.

The hazard is doubly covered. The floor argument is sound and the at-entry value is benign even without it. No change needed; flagged as fragile-only-in-principle in `02` (a one-line comment would help a future reader who edits the floor).
