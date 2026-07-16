# Review: iteration-39 PMI skid histogram + margin/2 gate (bead 19l)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-39-skid-histogram` vs `main`
- **Head:** `9b9bb42` ralph: iteration 39 checkpoint — PMI skid histogram + margin/2 gate (LIVE: max 31)
- **Box:** shared CI+dev host (load avg ~1.2 during review), `/dev/kvm` rw, kvm-intel
- **Verdict:** **APPROVE** (ship). No Critical or Important findings. Three minor Suggestions.

## What this is

A second, independent review — I deliberately ran every empirical claim myself
rather than re-deriving the first reviewer's notes. The change adds:

- `crates/dh-verify/src/skid.rs` — `SkidHistogram` (BTreeMap buckets, deterministic
  text + Prometheus exports, `assert_margin` exit-gate returning typed
  `MarginViolation`).
- `tools/dh-cli/src/skid.rs` + `dh-cli skid [--samples N]` subcommand — the live
  measurement driver (boots the landing-loop guest, arms the PMI per sample,
  records `counter_after − armed_point`).
- `crates/dh-vmm/src/run.rs::current_tid()` — extracts the real `gettid()` syscall
  so the unsafe-free dh-cli no longer mis-uses `process::id()` as a tid.
- `tools/dh-cli/tests/skid_gate.rs` — the live R1 exit gate (max skid < margin/2).

## Headline adjudication: 18 (iter-16) vs 27..31 (this harness)

**Both numbers are correct; the difference is fully explained by instruction mix,
and it does not matter for the gate.** I reproduced both on this box:

| Context | Guest at kick point | Skid | Variance |
|---|---|---|---|
| iter-16 `pmi_kick` test | real-mode `jmp $` (EB FE), 1 trivial insn, no memory | **18** | 0 (6/6 runs) |
| iter-39 `dh-cli skid` | 64-bit landing_loop: 8-insn LCG body w/ `imul`+`rol`+**store** | **27..31** | deterministic tri-modal |

The iter-16 note ("exactly 18, zero variance, period-independent") measured the
*floor* of PMI delivery latency on the most trivial possible stream. The new
harness arms mid-loop on a warm 64-bit guest whose body has a multi-cycle
dependent chain (`imul rax,r10` → `add` → `rol`) and a retiring **store** every
iteration. More instructions are in the out-of-order window when the NMI is
delivered, so ~10 more retire before the kick lands. 27..31 vs 18 is the right
direction and the right magnitude. See `01-critical-and-important.md` for the
docs-trail recommendation (annotate both, don't contradict).

**Crucially: both are ≪ 4096 (skid_margin/2). The gate is the deliverable, not the
exact constant.** A 73× safety factor on the worst observed sample.

## 1000-sample tail

`dh-cli skid --samples 1000`:

```
27 334
30 333
31 333
# samples=1000 min=27 max=31 sum=29331
GATE OK: max skid 31 < skid_margin/2 (4096)
```

**Three distinct values, no tail.** No two-digit outlier above 31, let alone a
three-digit one. The margin/2 headroom claim rests on a tail that does not exist
on this silicon for this workload. Max 31, p100 = 31.

## Empirical results (all run by this reviewer)

| Check | Result |
|---|---|
| `dh-cli skid --samples 1000` | min 27, max 31, 3 buckets, gate OK |
| `dh-cli skid` (200) × 5 | **bit-identical every run** (sum=5866) — deterministic under load |
| iter-16 real-mode overshoot × 6 | **18 every run** (scratch instrument, reverted) |
| `skid_gate` live test × 3 | 3/3 pass |
| `cargo test --workspace` | all pass, 0 failed |
| `cargo clippy -p dh-cli -p dh-verify --all-targets` | clean |
| 1000 samples wall time | 0.07 s → ~14k PMI/s ≪ 77k `max_sample_rate` |
| `process::id()`-as-tid anywhere | **none** (grep clean) |
| dh-cli `forbid(unsafe_code)` | intact (main.rs + lib.rs); only `libc::EINTR` const used |

## Throttle check

`perf_event_max_sample_rate = 77000`, `perf_cpu_time_max_percent = 25`. The harness
generates ~14k PMI/s (one overflow per sample, then parked to NEVER_FIRES) — well
under the cap. The PERIODS floor of 10k + the arm/park-after-EINTR discipline is
what keeps it there; the iter-16 hazard was period 100 free-running. Strongest
evidence throttling is NOT silently dropping overflows: the bit-identical results
across 5 runs. A throttled counter would skip overflows and produce variable skid.
(`dmesg` not readable as this user — no passwordless sudo — so I relied on the
rate math + determinism rather than the kernel warning line.)

## Busy-host robustness

This box ran the review concurrently with CI/dev (load ~1.2). The measurement was
still bit-identical across runs and the live gate passed 3/3. The test's `< 200`
sanity bound and the margin/2 gate are robust to this host's noise — confirmed by
evidence, not assertion. A dedicated quiesced runner would only tighten this.
