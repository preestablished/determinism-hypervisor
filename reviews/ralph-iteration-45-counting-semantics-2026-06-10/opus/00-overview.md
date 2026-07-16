# Iteration 45 — counting_semantics — Adversarial Review (Overview)

- **Branch:** `ralph/iteration-45-counting-semantics`
- **Head:** `874f570` (iteration 45 checkpoint)
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (adversarial, execution-driven)
- **Lab box:** Intel i5-8400 (Coffee Lake), `/dev/kvm` rw, `perf_event_paranoid=1`, nasm 2.16.01

## Scope

Bead d34: a known 1,000-instruction guest sequence for the M2 §3.1 counting
empirics, plus the empirical CLAIM that VM-exiting instructions retire ZERO
guest instructions under `exclude_host=1` — directly contradicting the
vendored ARCHITECTURE.md §3.1 ("CPUID, HLT, MMIO-exiting instructions each
retire exactly once, on the resume that completes them"). Reconciliation
tracked in bead 0sc.

Files reviewed:
- `tests/nanokernel/asm/counting.asm` (NEW)
- `tests/nanokernel/build.rs`, `tests/nanokernel/src/lib.rs`
- `tests/determinism/tests/counting_smoke.rs` (NEW, live)
- `crates/dh-vmm/src/{boundary,runctl,inject}.rs` (does any code depend on the wrong spec claim?)
- `crates/dh-detclock/src/counter.rs` (exclude_host config)

## What I did (execution-driven)

1. Built + ran `counting_smoke` — **passes, 5/5 reruns**, delta = 997, serial `SME`.
2. **Designed and ran a decisive isolation experiment** (scratch guest
   `zzprobe.asm` + scratch test): each §3.1 construct bracketed by serial-OUT
   markers, counter read at each OUT exit, inter-OUT deltas isolating each
   construct's retirement. Cleaned up afterward (git tree verified clean).
3. Disassembled `counting.elf`; reproduced the assembler `ICOUNT` arithmetic
   independently (26 before PAD + 974 PAD = 1000).
4. Audited boundary/runctl/inject for any latent "+1 for an exiting
   instruction" assumption.
5. Full workspace `cargo test` (all pass), clippy x86_64 (clean), clippy
   aarch64 cross (clean).

## Isolation experiment result (the core finding)

Measured inter-OUT deltas, model `delta = retire(prev OUT) + non_exiting_filler + retire(construct) + 2 (MARK setup)`:

| window         | filler | delta | conclusion                |
|----------------|--------|-------|---------------------------|
| OUT→OUT (10 add)| 10    | 12    | **OUT retires 0**         |
| OUT→OUT (10 add)| 10    | 12    | reproducible (OUT = 0)    |
| CPUID (xor+cpuid)| 1    | 3     | **CPUID retires 0**       |
| REP MOVSB (3+rep)| 3    | 6     | **REP MOVSB retires 1**   |
| MMIO read (mov+rd)| 1   | 3     | **MMIO read retires 0**   |
| MMIO write (mov+wr)| 1  | 3     | **MMIO write retires 0**  |

Deltas bit-identical across cold boots. A pre-existing reviewer scratch
(`zz_scratch_counting_probe.rs`, left untracked from a prior session — I ran
and then removed it) independently shows `s=6, e=1003, delta=997` with a
20-boot histogram `{997: 20}`.

**The 997 decomposition is the UNIQUE consistent explanation.** The region is
exactly 1,000 executed instructions; exactly 3 of them VM-exit (CPUID, MMIO
read, MMIO write) and each retires 0; everything else (incl. REP MOVSB → 1)
retires normally. 1000 − 3 = 997. The iteration's empirical claim is CORRECT
and the vendored §3.1 sentence is WRONG on this class. No alternative
decomposition (e.g. REP retiring 0) survives the isolation data.

## Verdict

**APPROVE**

The code is correct, the live test is sound, the determinism holds, and the
headline empirical claim is independently confirmed by isolation experiments.
The spec contradiction is real and correctly dispositioned as a doc bug (bead
0sc) — no code depends on the wrong "retires once" wording. Findings are
suggestions/polish only; none block merge.

## Stats

- Critical: 0
- Important: 0
- Suggestions: 5
- Positive notes: 7
- Tests run: counting_smoke (x6), full workspace (all pass), clippy x86_64 + aarch64 (clean), custom isolation guest (decisive)
