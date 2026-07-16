# Positive Notes

These docs are unusually trustworthy for operational truth. Highlights:

1. **dh-cli synopsis matches `usage()` byte-for-byte.** The README block
   (caps / cpuid-diff / boot / run / skid / gate) matches
   `tools/dh-cli/src/cli.rs::usage()` exactly — flag names, optionality
   markers `[...]`, the mutually-exclusive `(--icount-budget N | --vns-budget N)`
   group, and command order. A bad-arg invocation prints precisely this text and
   exits 2. `caps` really prints `kvm_m0_missing_caps=N` form.

2. **Every kvm-gated runtime is accurate.** regression 4.06s (≈4s),
   if0_deferral 31.7s (≈32s), landing_precision 64.7s (≈71s, within noise),
   timer_determinism 95.0s (≈95s, and the test name literally encodes "100
   runs"). These are not hand-waved estimates — they hold on the real box.

3. **The R2 / §3.1 narrative is precisely faithful to the vendored ARCH.**
   `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` §3.1 (lines 230–250)
   says VM-exiting instructions retire **zero**, MEASURED for "CPUID, PIO OUT,
   MMIO read, MMIO write, HLT", with **PIO IN explicitly EXPECTED-but-not-yet-
   isolated**. The README reproduces exactly this set, names "PIO OUT"
   specifically, and correctly does NOT claim PIO IN. It also faithfully records
   the "original spec said 'exactly once'; empirics refuted that" reversal and
   the `exclude_host=1`/host-side-RIP-skip mechanism. This is the subtle bit and
   it's right.

4. **TSC numbers are exact.** `docs/decisions/tsc-alignment.md` table: TSC_OFFSET
   device-attr = **932** ns, MSR-write = **1,107** ns (N=10,000, release). README's
   "932 ns vs 1107 ns" and the "device attr chosen over MSR-write restore"
   rationale match.

5. **Regression claim is exact.** `regression.rs` runs the landing loop for
   `BILLION = 1_000_000_000` instructions TWICE from cold boot and asserts the
   full state-hash chain identical — exactly README's "1e9 instructions ×2 from
   cold boot, state-hash chains identical, ~4s".

6. **The whole runbook is executable and green.** `apply-host-config.sh --verify`
   is genuinely read-only (root gate is only on the apply path, line 66–69),
   `dh-workerd --preflight` is the correct bin+arg and exits 0 with all §7.4/§2.1
   checks ok, and `check-determinism-class.sh` reports "7 keys" matching — and
   the box it reports (i5-8400 / 6.8.0-124 / 0xfa) is exactly the README header.

7. **Every referenced path exists** — CONTRIBUTING.md, both docs/ops files,
   docs/decisions/tsc-alignment.md, both .github workflows, ci script. No dead
   links.

8. **The self-skip framing is honest.** The matrix's "same command is correct
   everywhere; live legs self-skip when /dev/kvm not usable" is borne out by the
   `kvm_usable()` guards in the test sources.

9. **Tree clean, clippy clean, full `cargo test --workspace` green** after all
   verification.
