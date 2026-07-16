# Positive Notes

## The RCX-as-detector design is genuinely clever

`rep_loop.asm` makes the *guest's own register state* the mid-REP oracle: RCX is 64 exactly at the REP-MOVSB start and 0 everywhere else in the 6-instruction loop, because `lea/lea/add/jmp` never touch RCX and a completed REP leaves it 0. Any value in `(0, 64)` at a landed boundary is an unambiguous §3.2 violation, detectable with a single equality check and no host-side bookkeeping. The probe (icount 1..20) confirmed the encoding behaves exactly as the asm comment claims: 64 only at the REP start, 0 at every other instruction start, and the loop's 6-instruction period is visible in the RIP cycle (0x100020 → 0x100035@rcx=64 → 0x10003b → wrap).

## Zero overshoots across 10,000 + 1,000 targets, twice each — and it actually ran

This is a real, executing P0 acceptance. 10,000 distinct sorted targets landed at margin 256 (boot A bulk) and re-landed at 128 (boot B), with `icount == target` asserted on every one and full `Vec<Boundary>` equality across boots. Plus 1,000 REP-loop targets with the same scheme. All passed in one run (93s here). No `Overshoot`, no unexpected exit, no mid-REP boundary.

## Margin-independence is real and stronger than the test asserts

My scratch experiment landed identical targets at {8192/1024, 4096/512, 64/64} — a 128x margin spread across the full [1000, 99M) range — and got bit-identical tuples. The §3.2 "result guaranteed margin-independent" contract is not just asserted, it survives a deliberately adversarial spread that the shipped test does not itself apply on large targets.

## Determinism / no-host-randomness discipline

Targets come from a fixed-seed SplitMix64 with the standard constants (`0x9E3779B97F4A7C15`, `0xBF58476D1CE4E5B9`, `0x94D049BB133111EB`), distinct-and-sorted via `BTreeSet`. No host RNG on the test path, no time-dependence, no env-dependence. Seeds are hardcoded per test. The whole module compiles to empty off x86_64 (`#![cfg(target_arch = "x86_64")]`), and aarch64 clippy is clean.

## resync_slack is correctly a landing-only knob

Audited `land_at`: `resync_slack` is referenced at exactly one site (`boundary.rs:137`) — the far-vs-near approach hysteresis (`d > skid_margin + resync_slack`). It never touches the landed boundary, only the approach strategy. The test's per-boot slack values (256 then 128) change *how* the engine lands, not *where* — exactly matching the §3.2 "margins are landing-only" claim and the module's own doc.

## Skid headroom at the chosen margins is comfortable

50k samples: max skid 39, 99.998% ≤ 31. Boot B margin 128 (alert at 64) has 1.6x headroom over the worst observed skid and ~3.3x over the typical max; overshoot would require skid > 128. The tight margins are well justified — there was no need to bump boot B to 192/256.

## Ceiling safety is numerically sound

`LANDING_CEILING = 99_000_000` keeps every landing target ≥ ~1M below the guest's completion 'L'-OUT at 100M + prologue. The 8192 production margin only ever applies to the 100 *smallest* targets (sorted prefix), so the far-arm at `target − 8192` can never overrun the guest's terminal sequence. The endless `rep_loop` (ceiling 3M) is trivially safe — it never terminates.

## Honest, accurate documentation

The module headers, the asm comments, and `lib.rs` accessors all match the runtime behavior I observed (instruction counts, the 6-per-iter REP loop accounting, the RCX semantics). The `counting.rs`-adjacent §3.1 "VM-exiting instrs retire zero" note in `lib.rs` is consistent with this iteration's REP-counts-as-one accounting.
