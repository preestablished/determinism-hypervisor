# Review — counting-semantics nanokernel (bead d34)

- Branch: `ralph/iteration-45-counting-semantics`
- Base: `main` (diff `main...HEAD`, head `874f570`)
- Date: 2026-06-10
- Reviewer: Claude Opus (2nd reviewer)
- Verdict: **APPROVE**

## Summary

The change adds a 1,000-instruction-by-construction nanokernel guest
(`counting.asm`), `lib.rs` accessors/constants, a `build.rs` registration,
and a live smoke (`counting_smoke.rs`) that proves the marker-window counter
delta is exactly **997** across two cold boots. The "1,000 instructions" is
enforced at *assembly time* by the `I` macro counter and an `%if … %error`
gate, so the guest cannot be built wrong. The 997 figure is `1000 − 3` (three
in-region VM-exiting instructions: CPUID, the pv-clock MMIO read, the
serial-THR MMIO write), encoding the measured §3.1 empiric that exiting
instructions retire **zero** under the `exclude_host=1` guest counter —
contradicting the vendored spec's "retire exactly once on the completing
resume." That reconciliation is correctly filed as bead `0sc`.

I ran the code rather than eyeballing it. Everything checks out: the static
disassembly matches the macro accounting exactly, the live count is bit-stable
under every perturbation I could throw at it, and — importantly for a
deterministic hypervisor — the 997-vs-1000 discrepancy is **purely an
attribution/documentation issue**: the boundary engine (`land_at`) never
predicts a +1 from any instruction; it treats `counter.read()` as the only
source of truth. So there is no latent landing bug hiding behind the spec
contradiction.

The one thing worth tightening before this milestone closes is the **scope of
bead `0sc`**: it covers the ARCH §3.1 prose and the boundary-engine wording,
but it does NOT mention that bead **`gfb`** (the P0 M2 acceptance) still
states in its own description "counter delta exactly 1,000" and "CPUID/HLT/
MMIO-exiting instructions retire exactly once." A fresh implementer of `gfb`
would code to the wrong expectation. This is a follow-up bead concern, not a
blocker on this diff — hence APPROVE, with one Important action item.

## Verification performed (on the Intel lab box, ran live)

- `cargo test -p determinism-tests --test counting_smoke` → pass.
- Disassembled the built ELF; counted region instructions: **994 static
  emitted**, **1,000 dynamic** (the 4× dec/jnz loop accounts for +6). Matches
  the macro accounting (`ICOUNT == 1000`, PAD = 974 `add` filler) exactly.
- 12 fresh test-binary processes (cold) → 12/12 pass.
- Instrumented scratch probe, 20 cold boots in one process → delta `{997: 20}`,
  final HLT icount `{1004: 20}`, raw `s=6 e=1003` every time, serial `[S,M,E]`.
- `taskset -c {0..5}` (one per physical core; box is SMT-off, 6c/6t) → 997 on
  every core.
- Under full CPU load (6 spin burners) + `nice -n 19`, both pinned to a
  contended core and free-migrating → `{997: 8}` both ways.
- aarch64 cross lane: `cargo check -p nanokernel --target
  aarch64-unknown-linux-gnu` (clang/llvm-ar-18/`/tmp/a64inc`) → clean; the
  arm build had already produced `counting.elf`.
- `cargo fmt --check` clean; `cargo clippy` clean (nanokernel + test crate);
  `cargo test -p nanokernel` 5/5.

## Stats

- Files changed: 4 (+327 / −0)
- New guest: `tests/nanokernel/asm/counting.asm` (120 lines)
- New test: `tests/determinism/tests/counting_smoke.rs` (165 lines)
- Findings: 0 Critical, 1 Important, 4 Suggestions
