# Critical & Important Findings

## Critical

None.

---

## Important

### I1. `dh-cli gate` (the bead-ksx command) is not invoked anywhere in CI — only the unit tests and the two `#[test]` functions run on the box

**File:** `.github/workflows/ci.yaml` (kvm-intel lane, line 107: `cargo test --workspace`);
`tools/dh-cli/src/gate.rs`.

The kvm-intel lane runs `cargo test --workspace`, which automatically picks up the two new
integration tests (`timer_determinism`, `if0_deferral`) at their full 100 runs — this is where
the ~+130s comes from. That correctly covers beads 0zh and 3t9.

But the **`dh-cli gate` subcommand** — the one-command Phase-1 determinism gate that bead ksx
and the phase-1 doc "Exit gate" item 1 describe as *the* deliverable ("One command: boot
nanokernel, run to icount N twice → compare; repeat with a timer event; 100 consecutive runs")
— is exercised in CI only by its `dh-verify::gate` **unit tests** (the pure harness with fake
fingerprints). The live `run_gate` path (200 cold boots, the actual plain-landing + timer-event
sweep) is **never run by CI**. It only runs when a human types `dh-cli gate`.

This is not a correctness defect — I ran `dh-cli gate --runs 3` live and it passes, and the two
integration tests cover the same VM machinery through a different entry point. But it means the
headline artifact of bead ksx (the report-emitting command, exit code 1 on FAIL) has no
automated regression guard. If someone later breaks `run_gate`'s wiring (wrong ELF selection,
a fingerprint field dropped, the `--runs` default), CI stays green.

The bead text and phase doc frame the *command* as the gate. Two reasonable resolutions:

1. Add a smoke invocation to the kvm-intel lane: `cargo run -p dh-cli -- gate --runs 5` as a
   workflow step (≈3s), asserting exit 0. Cheap, directly guards the command.
2. Or convert `run_gate` into a `#[test]` (small N) in `tools/dh-cli/tests/` so `cargo test
   --workspace` covers it.

Either is a few lines. I'd file it as a follow-up bead rather than block the merge, since the
underlying determinism property *is* guarded by 0zh/3t9. But the gap is worth recording because
the bead's named deliverable is the command, not the harness.

**Why it matters:** the autopilot's "the M3 gate IS the product" framing only holds if the
product (the command) is itself under test. Right now the command is a manually-run convenience
wrapper over machinery that CI happens to test elsewhere.
