# Suggestions (non-blocking)

## S1. "max 39 over 50,000 samples" — note the stochastic outlier behaviour
**File:** README.md, "Measured numbers" → PMI skid bullet.
The claim is correctly framed as a *measurement* ("max 39 ... typical 18–31"),
not a guarantee — good. During this review, three fresh `skid --samples 50000`
runs gave **max 81, then 33, then 31**. The 81 was a transient (background load
on the box) and the gate still passed comfortably (81 ≪ skid_margin/2 = 4096).

No correctness problem: the design tolerates outliers by gating at margin/2, and
mean (~29) sits squarely in the typical band. But "max 39" can read as a tight
empirical ceiling. Consider softening to "max ~39 in a quiet 50k run (transient
spikes to ~80 under host load observed; gate headroom absorbs them — alerts at
margin/2 = 4096)". This pre-empts a future operator panicking when their run
shows 70+.

## S2. Matrix is a highlight table, not exhaustive — two integration tests live only under the workspace catch-all
**File:** docs/ops/test-partitioning.md.
The matrix explicitly states "Everything is part of `cargo test --workspace`",
which is the correct escape hatch. But two integration test files are not named
in any row and only run via the catch-all:
- `crates/dh-devices/tests/detguest_host_smoke.rs`
- `crates/dh-worker/tests/arch_dependency_rule.rs`
(`tests/nanokernel/tests/elf_shape.rs` *is* covered by the "`cargo test -p
nanokernel`" row since it runs all that crate's tests.)

Both are host-runnable unit/contract tests and fairly fall under "All pure unit
tests | `cargo test --workspace`". No action strictly required. If you want the
matrix to double as a coverage ledger, add a one-line row for the dh-worker
arch-dependency-rule test (it's a normative dependency guard worth surfacing).

## S3. `dh-cli run` synopsis trailing "..."
**File:** README.md dh-cli reference, `run` line ends with `...`.
The real usage is `run <guest.elf> (--icount-budget N | --vns-budget N)
[--mem-mib N] [--cmdline S]`. The README's `...` is a reasonable shorthand and
matches the spirit, but for parity with the other lines (which spell out all
flags) consider listing `[--mem-mib N] [--cmdline S]` explicitly. Cosmetic.

## S4. Gate runtime in the matrix ("~32s at 100 runs") not independently retimed
The matrix lists the one-command gate at "~32s at 100 runs". I verified the
gate *runs and passes* (`gate --runs 3`) and that its structure is 2×N
fingerprint boots (plain + timer, default 100). I did not retime a full 100-run
gate to the second. The figure is plausible given per-boot timings, but it is
the one number in the matrix I did not independently confirm at the stated run
count. Low risk.
