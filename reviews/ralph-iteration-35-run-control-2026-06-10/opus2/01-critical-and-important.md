# Critical and Important findings

## CRITICAL — Multi-vector injection at one boundary loses all but the last vector (PROVEN LIVE)

**Where:** `crates/dh-vmm/src/runctl.rs:194-212` (the inner chaining loop), against `crates/dh-vmm/src/inject.rs:126-160` (`inject_at_boundary`) and the explicit CONTRACT at `inject.rs:96-99`.

**The seam.** `agenda::StopPoint.injections` is a `Vec<usize>` — several scheduled vectors can share one boundary icount (e.g. two pv-timers arming the same instant). `agenda.rs:78-82` deliberately punts the multi-vector handling to run control:

> "ARCH §3.4 delivers ONE vector per VM entry, so run control must chain them across consecutive entries at this boundary."

`run_segment` attempts that chaining (runctl.rs:195-212): it loops `point.injections`, calls `inject_at_boundary` for each, and advances `at` to the returned `delivered_icount`. The intent is that each injection causes a VM entry before the next.

**Why it breaks.** `inject_at_boundary` only enters the guest (`land_at`) on the **deferral** path (`inject.rs:156`, when `injectable()` is false). When the boundary is **already injectable**, it queues immediately and returns with `delivered_icount == at.icount` (`inject.rs:138-145`) — **no KVM_RUN happened**. Back in runctl, `at` is set to the same icount (runctl.rs:207-211), so the *second* `inject_at_boundary` re-checks the **stale** kvm_run (still `injectable==1`, because no entry refreshed it), queues immediately too, and never steps. Result: two `KVM_INTERRUPT` ioctls with no KVM_RUN between them — exactly the overwrite the `inject.rs:96-99` CONTRACT warns against.

**Live proof (scratch, reverted).** On the `sti_window` guest with the window forced open:

1. Direct: `queue_interrupt(0x31)` → `KVM_GET_VCPU_EVENTS` shows `interrupt.injected=1, nr=0x31`. Then `queue_interrupt(0x32)` with **no** KVM_RUN → events show `injected=1, nr=0x32`. The 0x31 vector is gone. (`SCRATCH-OVERWRITE`)
2. Reproducing runctl's exact chaining loop body over `[0x31, 0x32]`: both returned `delivered_icount=4` (same boundary), and the final KVM queue held **only `nr=0x32`**. (`SCRATCH-CHAIN: delivered_icounts=[4, 4] ; final queue injected=1 nr=0x32`)

**Impact.** Wrong-result, silent. The lost vector is never delivered to the guest, yet `delivered += 1` runs for each (runctl.rs:206), so `injections_delivered` reports 2 when 1 actually queues. Determinism is preserved (it loses the *same* vector every replay), so verification mode would NOT catch it as a divergence — it would only show a guest that behaved as if one interrupt never fired, which is correct-but-wrong. This is the worst failure class: deterministically incorrect.

**Why latent now.** No Phase-1 path schedules injections: `dh-cli run` always passes `injections: &[]` (run.rs:658), and the runctl tests use `injections: &[]`. The bug is dormant until the M6 timer/SDK scheduler (or any caller) supplies two vectors at one icount. The agenda layer already *produces* multi-index stop points (`agenda.rs` `merges_all_sources_sorted` test builds them), so the trigger is one caller away.

**Fix options (recommend A):**

- **(A) Force a VM entry between queued vectors.** After queuing vector *i* in the chain, step the guest exactly +1 instruction (`land_at(at.icount + 1)`) before processing vector *i+1*. The first queued vector then delivers on that entry, and the next `inject_at_boundary` sees a fresh kvm_run. Note: with an interrupt delivered, the *next* injectable boundary is genuinely later, so `delivered_icount` values will differ per vector — which is the correct §3.4 semantics ("first injectable boundary ≥ B" applied sequentially as each prior interrupt consumes the window).
- **(B)** Have `inject_at_boundary` itself take the whole vector slice for a boundary and manage the inter-vector entries internally, keeping the "one vector per entry" invariant owned by the module that documents it.

Either way: add a live regression that schedules two vectors at one boundary on `sti_window` and asserts both deliver (e.g. two distinct observable effects, or `injected.nr` transitions across entries) — the current test suite has **zero** multi-vector coverage, which is why this shipped.

**Tighten the over-count regardless:** `injections_delivered` should count actual queue-and-entry events, not loop iterations.

---

## IMPORTANT — Guest HLT mid-run is a fatal "unexpected exit", not `GUEST_HALTED`

**Where:** `tools/dh-cli/src/run.rs:661-667` (`on_exit` closure) and `crates/dh-vmm/src/runctl.rs:73-78` (`StopReason` enum). Proto reference: API.md §2.4 `StopReason.GUEST_HALTED = 6` ("guest executed terminal HLT / triple fault").

**Observed live.** `dh-cli run pipeline_smoke --icount-budget 1000`:
```
dh-cli run: boundary: exit handler: unexpected exit: Hlt
EXITCODE=1
```
`pipeline_smoke` OUTs 'K' to the serial port then returns to crt0's HLT park (around icount 16-17 on this build — confirmed: budget=15 stops at rip `0x100016` with `serial:"K"`; budget≥~17 hits HLT). Because dh-cli's `on_exit` only recognizes serial OUT (run.rs:662-665) and maps everything else to `BoundaryError::Exit`, the HLT aborts the whole run: no JSON, exit code 1, **and the already-captured serial 'K' is discarded** (the report is never built).

**Why it matters.** `boundary.rs:95-99` is explicit that HLT/Shutdown handling is "run control's call, not this engine's." The proto has a first-class `GUEST_HALTED` stop reason for exactly this terminal case. The current code: (1) cannot represent it — `runctl::StopReason` has only `BudgetReached/GoalSatisfied/HardCap/Paused`; and (2) loses partial output on the error path. Any well-behaved guest that finishes its work and halts before exhausting the budget is reported as a hard error.

**Acceptable for the *checkpoint*?** Marginally — the M3 run-twice-compare driver works on the non-halting `landing_loop`. But it's a latent foot-gun: the landing budget for any halting guest must be tuned to stop *strictly before* HLT or the run "fails," and there is no documentation saying so.

**Fix options (recommend A for the checkpoint, B for M6):**

- **(A, doc + cheap)** Document in `runctl.rs` / `run.rs` that Phase-1 treats Hlt/Shutdown as caller-fatal and the budget must land before a terminal HLT; keep the behavior. Cheapest path to "honest."
- **(B, proper)** Add `StopReason::GuestHalted` to `runctl::StopReason`, have `run_segment` (or a designated `on_exit` contract) recognize `VcpuExit::Hlt`/`Shutdown` as a terminal stop, finish the segment (hash the boundary, return the captured serial), and map it to proto `GUEST_HALTED`. This is the §2.4-aligned behavior and what M6's gRPC Run needs anyway. File a bead.

Either way, **do not silently drop captured serial on the error path** — at minimum print what was captured before erroring.
