# Positive notes

- **Determinism delivered, proven live.** `dh-cli run landing_loop --icount-budget 500000` twice produced byte-identical `{icount, rip, vns, state_hash}` (hash `5398e78d6fa1cfd6c261a98e0230639ac77189936d9f81bb703f8747baa06fbf`). The run-twice-compare driver — the whole point of the M3 slice — works end to end through real KVM. The 4 `runctl` live tests passed 3× with zero flakes.

- **Clean enum/agenda layering.** `Until` → `FinalStop` + `goal_poll_period` mapping (runctl.rs:155-167) is a faithful, 1:1-with-proto translation. `NotYetWired(&'static str)` for `NextSdkEvent`/`FrameBudget` is exactly right: the enum shape matches API.md §2.4 so M6's gRPC Run maps directly, and the unwired modes fail loudly (test `unwired_modes_fail_loudly`). Margins sourced from `MachineConfig` (single source of truth, bead srz) rather than re-defaulted.

- **Pause roll-forward is correct and well-tested.** `next_epoch = point.icount.div_ceil(epoch).max(1) * epoch` (runctl.rs:240-241) lands the pause on the deterministic grid as ARCH §3.3 / API.md §2.4 require, and `pause_rolls_forward_to_the_epoch_boundary_live` asserts both grid-alignment and the ≤ epoch_len latency bound. The "external async input, not replayed" semantics are correctly understood.

- **`vns-budget` works through the clock rational.** `dh-cli run landing_loop --vns-budget 300000` stopped at icount=vns=300000 (clock 1:1), exercising the `FinalStop::VnsBudget` → `icount_for_vns_target` conversion path in the agenda live.

- **Honest scoping and overflow discipline.** `ClockOverflow` / `BudgetOverflow` are propagated, not unwrapped. `agenda::compile` is genuinely pure and has a 2000-case property test including u64-edge cases. The runctl loop ends in `unreachable!("agenda always carries exactly one final stop point")` — backed by the agenda invariant the property test enforces (exactly one `final_stop`).

- **Serial capture is correct when the run stops cleanly.** At `--icount-budget 15` the `pipeline_smoke` 'K' byte was captured and surfaced in the JSON (`serial:"K"`), proving the `on_exit` serial path (run.rs:662-665) and `json_escape` work. (The loss only happens on the HLT error path — see Important finding.)

- **`inject.rs` already documents the exact hazard this review's Critical exploits.** The CONTRACT comment at `inject.rs:96-99` ("a second KVM_INTERRUPT before the next KVM_RUN silently OVERWRITES the first… Run control must enter the guest between injections") is precise and was clearly hard-won (live-reproduced in a prior review). The defect is purely that runctl didn't honor it — the knowledge is captured where it belongs.

- **Idempotent setup.** `install_kick_handler` is called per `run()` (run.rs:622) and is idempotent; the counter is dropped at function end; no global state leaks across CLI invocations (consistent with the byte-identical run-twice result).
