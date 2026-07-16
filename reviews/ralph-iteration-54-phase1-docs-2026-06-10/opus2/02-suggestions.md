# Suggestions

## S-1 — Hedge the macOS host-runnable claim (audience angle 3)

`docs/ops/test-partitioning.md` lists, under "Host-runnable (any machine —
macOS, aarch64, ...)", the nanokernel ELF/asm tests that depend on
`tests/nanokernel/build.rs`. I reasoned through that build.rs's linker chain
for a Mach-O host:

- `nasm -f elf64` cross-assembles x86 ELF objects on any host — fine on macOS.
- `find_linker()` probes, in order: GNU `ld` with `-m elf_x86_64` (macOS system
  `ld` is Mach-O-only and rejects `-m elf_x86_64`, so it correctly falls
  through via the `probe()` `--version` guard), then `ld.lld`/`lld` (only if
  Homebrew LLVM is installed), then **rust-lld inside the Rust sysroot**
  (`lib/rustlib/<host>/bin/rust-lld`, invoked with `-flavor gnu -m elf_x86_64`).
- rust-lld is a cross-linker and *does* emit ELF regardless of host OS, so the
  chain **can** succeed on an Apple-silicon or Intel Mac with a stock Rust
  toolchain.

So the claim is **plausible and the build.rs is genuinely designed for it.**
But: nothing in CI exercises a macOS host (the matrix is ubuntu x86 + ubuntu
arm only), so this path is **unverified**. The doc states macOS flatly in a
parenthetical that reads as a guarantee. Recommend a one-line hedge, e.g. a
footnote on the nanokernel row: "macOS: builds via the rust-lld sysroot
fallback (system `ld` is Mach-O-only and is skipped); expected to work,
not exercised in CI." This keeps the honest-everywhere spirit without
promising an untested platform.

## S-2 — Add a measurement date to the README "Measured numbers" heading (staleness, angle 5)

The heading names the box/kernel/ucode ("lab box: i5-8400, kernel 6.8.0-124,
ucode 0xfa") — good provenance for *which host*. But these numbers will rot on
the next kernel/microcode bump, and the heading has no **date**, whereas its
sibling docs do: `docs/decisions/tsc-alignment.md` dates its measurements
(2026-06-10) and `ci/determinism-class.lock` dates its baseline (2026-06-09,
re-baselined same day). Suggest "(measured 2026-06-10; lab box: i5-8400 ...)"
so a future reader can tell whether the numbers predate a re-baseline. Matches
the rest of the doc set's convention.

## S-3 — README "runs in CI on every kernel/microcode bump" is true but worth tightening

The R2 section says `counting_semantics` "runs in CI on every kernel/microcode
bump." I verified the wiring: it's in the per-push kvm-intel lane (via
`cargo test --workspace`, ci.yaml line 108) AND explicitly in nightly-drift
(nightly-drift.yaml line 46). So the *test* runs on every push and every night.
But "on every kernel/microcode bump" is slightly imprecise — a bump is caught by
the nightly **drift** check failing (the lock comparison), which is what *forces*
the re-baseline that then re-runs counting_semantics. The test doesn't trigger
*on* the bump; the drift tripwire does. Minor wording: "runs per-push and
nightly; a kernel/microcode bump is caught by the nightly drift check, whose
re-baseline procedure re-runs it." Optional.

## S-4 — Stale comment in ci/determinism-class.lock (out of this diff, flag for follow-up)

Not part of this diff, but surfaced while verifying angle 2: the lock file's
parse-contract comment says "for the nightly comparator, **which does not exist
yet**." It now exists (`nightly-drift.yaml` runs `check-determinism-class.sh`).
Worth a one-word fix in a future iteration so the doc set doesn't carry a
self-contradiction (CONTRIBUTING.md + the runbook both describe the nightly as
live). File a bead.
