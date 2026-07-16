# Action Items

Branch `ralph/iteration-34-interrupt-injection` (bead determinism-hypervisor-mny).
Verdict: **APPROVE** — no blocking items.

### Critical

_None._

### Important

_None._

### Suggestions

All optional, low-risk, may land in this iteration or a follow-up bead.

1. **Guard exception-vector range in `queue_interrupt`** —
   `crates/dh-vmm/src/inject.rs`, `queue_interrupt`. Reject `vector < 32` with a
   loud `InjectError::Kvm` before issuing the ioctl, since `0..32` are CPU
   exception vectors and never valid as external-interrupt vectors. Turns a
   caller mistake into a deterministic error. Tests use `0x30` and are
   unaffected.

2. **Make the "freshly landed" precondition self-checking** —
   `crates/dh-vmm/src/inject.rs`, `inject_at_boundary`. The first `injectable()`
   read trusts the current exit-time `kvm_run` summaries, which are only valid
   immediately after the `land_at(B)` that produced `at`. Either assert
   `counter.read()? == at.icount` at entry (loud, matches the project's
   Overshoot-style stale-input guard) or add a doc note that nothing may run
   between producing `at` and calling this. Functionally benign today (a stale
   `at` just costs one deferral step), so non-blocking.

3. **Document NMI / KVM_INTERRUPT queue independence** —
   `crates/dh-vmm/src/inject.rs`, `injectable()`. One-line comment: NMIs use the
   separate KVM_NMI queue and do not gate KVM_INTERRUPT in the userspace-irqchip
   regime, so the "no pending exception" check intentionally does not inspect
   NMI-pending. Pre-empts the question and documents scope vs §3.4.

4. **(Perf — defer until profiling demands)** Per-step single-step
   enable/disable churn in the deferral loop (2 ioctls/step via repeated
   `land_at(+1)`). Fine at tested scale (250 steps, 0.67 s suite). Only if a
   future workload shows long deferrals as a hotspot, consider an inject-local
   stepping loop that keeps single-step armed across re-checks. File a perf bead
   only when measurements justify it — do not pre-optimize.

### Follow-ups (tracked elsewhere, not this iteration)

- **Test the mid-deferral window OPEN** (guest executes STI during stepping so
  the window opens *between* `at` and delivery). Both current tests are
  endpoints: window-closed-forever (deferral cap) and window-already-open
  (zero-step delivery). The intermediate case — deferral that genuinely resolves
  partway — needs a guest that runs STI under controlled icount, which depends
  on bead-583's guest. Note as the next test to add; **not a blocker** for this
  rule, whose determinism argument and endpoints are already proven.

- **Wire `inject_at_boundary` into run control.** The module has no in-tree
  caller yet; the agenda already models injection stop points
  (`agenda.rs` `StopPoint.injections`) and the AUX record field exists
  (`dh-inputlog/src/dhilog.rs:210` `delivered_icount`). Integration (call
  `inject_at_boundary` at each agenda injection boundary and emit the AUX
  `TIMER_FIRE` record from the returned `Injection`) is the natural next bead.
