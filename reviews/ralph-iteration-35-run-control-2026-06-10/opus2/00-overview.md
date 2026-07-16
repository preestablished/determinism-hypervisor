# Review: Phase-1 run control (`runctl.rs` + `dh-cli run`)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-35-run-control` vs `main`
- **Bead:** qs4
- **Scope:** `crates/dh-vmm/src/runctl.rs` (new, 446 lines), `tools/dh-cli/src/run.rs` (new, 98 lines), `tools/dh-cli/src/main.rs` (`run_cmd`), wiring (`lib.rs`, `Cargo.{toml,lock}`)
- **Method:** Independent composition-bug hunt at the seams between proven crates (`agenda`, `inject`, `boundary`, `hash`). Live experiments against `/dev/kvm` (rw available); scratch tests reverted.

## Verdict

**APPROVE WITH CHANGES.** The Phase-1 happy path is correct and provably deterministic — run-twice-compare on `landing_loop` produced byte-identical icount/rip/vns/state_hash live. But two real composition defects live in the seams:

1. **CRITICAL — multi-vector injection overwrite (lost interrupt), PROVEN LIVE.** When an agenda `StopPoint` carries two injections at one boundary and that boundary is *already injectable*, `run_segment`'s chaining loop queues both vectors with **no KVM_RUN between them**, so the second `KVM_INTERRUPT` silently overwrites the first in the KVM vector queue. The first vector is lost; `injections_delivered` over-counts. This directly violates the CONTRACT comment `inject.rs:96-99` ("Run control must enter the guest between injections"). Latent in Phase-1 (no CLI path schedules injections) but a guaranteed wrong-result bug the moment the M6 timer/SDK scheduler feeds two coincident vectors.

2. **IMPORTANT — guest HLT mid-run is a fatal error, not a stop reason.** `dh-cli run` on `pipeline_smoke` (OUTs 'K', parks in HLT) with a budget past the HLT aborts with `exit handler: unexpected exit: Hlt`, exit code 1, no JSON, and the serial 'K' byte is lost. The proto defines `GUEST_HALTED = 6` precisely for this; `runctl::StopReason` omits it and the CLI cannot surface it.

Both are accompanied by smaller hardening/doc gaps (see 01/02). None block the *checkpoint*; (1) must be fixed or explicitly deferred-with-a-bead before the M6 scheduler lands, since that is the first caller that triggers it.

## Live verification performed (all on this host, /dev/kvm rw)

| Experiment | Result |
|---|---|
| `runctl` 4 tests × 3 runs | all pass, **no flakes** |
| Full `cargo test --workspace` | green (66 dh-vmm lib + all crates) |
| `dh-cli run landing_loop --icount-budget 500000` ×2 | **byte-identical** JSON (hash `5398e78d…`) |
| `dh-cli run landing_loop --vns-budget 300000` | ok, icount=vns=300000 |
| `dh-cli run pipeline_smoke --icount-budget {3,5,8,10,12,15}` | budget_reached; 'K' serial captured at b=15 |
| `dh-cli run pipeline_smoke --icount-budget 1000` (past HLT) | **fatal** `unexpected exit: Hlt`, rc=1 |
| Scratch: queue 0x31 then 0x32, no KVM_RUN | KVM events: `nr=0x31`→`nr=0x32` (**overwrite**) |
| Scratch: runctl chaining loop, 2 vectors @ same open boundary | both `delivered_icount=4`, queue holds **only 0x32** |

## Stats

- Files reviewed: 5 changed (+ 3 supporting crates read in full)
- Findings: 1 Critical, 1 Important, 6 Suggestions, plus positive notes
- Live experiments: 7 distinct, scratch reverted, tree clean
