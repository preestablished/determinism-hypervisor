# Review: ralph iteration 54 — Phase-1 docs cluster (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-54-phase1-docs
- **Scope:** docs-only (`git diff main...HEAD`): README.md (+47 as-built sections, bead hny), docs/ops/test-partitioning.md (+49 new, bead b0h)
- **Verdict:** REQUEST_CHANGES

## Summary

This is a strong, accurate as-built docs iteration. I verified every measured
number, every command, and every test-file name against the code on the lab
box (i5-8400 / 6.8.0-124 / ucode 0xfa, /dev/kvm present). They match. The
dh-cli subcommand list, the skid_margin=8192 / alert-at-margin/2 policy, the
932-vs-1107 TSC numbers, the R2 counting-semantics rule, the 7-key
determinism-class lock, and the `dh-workerd --preflight` runbook command all
check out and run clean. Tree clean after verification.

I am requesting changes for **two** issues that are small to fix but matter for
the stated audience (an agent on a non-Intel machine) and for technical
accuracy:

1. **CRITICAL (accuracy / dead pointer):** test-partitioning.md tells an
   off-arm dev to "see CI for the cross-cc env" for the aarch64 clippy command.
   There is no cross-cc env anywhere in CI or the repo — the arm lane runs
   *natively* on `ubuntu-24.04-arm`. The pointer resolves to nothing, and an
   x86 dev running the documented `--target aarch64-unknown-linux-gnu` command
   will hit a missing-linker/CC failure with no in-repo guidance. (Detail in
   01.)

2. **IMPORTANT (mischaracterized metric):** the README calls the TSC numbers
   "932 ns vs 1107 ns worst-case alignment error." The source
   (`docs/decisions/tsc-alignment.md`) measures them as **ns/call** ioctl
   latency, not alignment error. The MSR path's actual determinism hazard is
   sync-heuristic quantization, not a 1107 ns error. (Detail in 01.)

The macOS host-runnable claim (angle 3) is **plausible but should be hedged**
— see 02. Reasoning from build.rs: the rust-lld sysroot fallback does handle
ELF cross-linking from a Mac, so it *can* work, but nothing in the repo or CI
verifies a Mach-O host, and the doc promises it flatly. Recommend a "x86/arm
Linux verified in CI; macOS expected to work via the rust-lld fallback,
unverified" hedge rather than an unqualified promise.

See 01 (critical+important), 02 (suggestions), 03 (positive), 04 (action items).
