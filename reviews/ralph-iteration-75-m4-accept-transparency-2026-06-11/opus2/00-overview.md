# M4 Snapshot-Transparency Acceptance — Review (2nd Reviewer)

- **Branch:** `ralph/iteration-75-m4-accept-transparency`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 3 files, +286 / -0, 1 commit

## Summary

This change adds one live acceptance test (`crates/dh-worker/tests/m4_transparency.rs`,
bead 7c8) plus the dev-deps it needs (`kvm-ioctls`, `libc`, `nanokernel` path dep, and
the Cargo.lock delta). The test compares two legs of the landing-loop guest run to 2e8
instructions: a control leg (1e8 → pause → 1e8) against a roundtrip leg that takes a real
in-process-snapstore snapshot at 1e8, destroys the slot+bus, restores into a fresh
slot+bus, and runs the remaining 1e8. It asserts the §8.5 state-hash chains are bit-equal
(H1==H2) plus boundary/vns equality, and front-loads an `r1==c1` leg-equality assert so a
later mismatch is unambiguous. I verified the test's load-bearing assumptions against
runctl (IcountBudget lands exactly; epoch grid is absolute-counter-space; pause
roll-forward; final-link semantics), hash.rs (TSC normalized to vns, full-RAM walk,
canonical blob), restore/snapshot engines (counter:None keeps the shared axis; the
counter-reset path is genuinely covered in `restore_engine.rs`), tsc.rs, the counter's
`exclude_host` contract, and the nanokernel landing-loop (`b"30000000"` = 2.4e8 capacity,
8 instr/iter, no RDTSC/CPUID/RDRAND). The wiring is correct and the gate is meaningful.

The test is well-constructed and its in-code documentation is unusually careful. My
findings are about the **boundary of what this gate proves** (it cannot, by construction,
catch a raw-guest-TSC trajectory divergence or any device-state difference, because the
chain hashes `device_sections=&[]` and normalizes TSC to vns), one assertion that is
effectively **tautological** (`r2.vns == c2.vns`), and routine **cargo/maintainability
hygiene** (triplicated dep version literals; triplicated `test_bus`/`spawn_store_blocking`
helpers). None of these block the milestone — the documented scope is internally honest —
but the strongest claims in the module doc and the test's failure message overstate what a
green run demonstrates, and that is worth tightening before this becomes the reference
M4-accept gate.

## Verdict

**APPROVE** — with two doc-honesty fixes and the hygiene items recommended as fast
follow-ups (none are merge-blockers).
