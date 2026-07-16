# Action items

### Critical

- [ ] **C1 — Fix the `request_interrupt_window` / `IrqWindowOpen` composition gap.**
  `crates/dh-vmm/src/inject.rs:131-133`. As written, the moment a guest opens its
  interrupt window mid-deferral (e.g. STI to accept a timer), the next `land_at`
  single-step KVM_RUN returns `VcpuExit::IrqWindowOpen`, which `land_at` routes to
  `on_exit` and treats as fatal — the vector is never delivered. Reproduced live on this
  box (Intel, kernel 6.8). **Recommended fix:** delete the
  `request_interrupt_window = 1` set (line 131) and its clear (line 137); the stepped
  path re-checks `injectable()` every retirement on its own, so the only cost is one
  extra deterministic step past an STI shadow. If the line must stay for a future
  full-run deferral path, instead wrap `on_exit` so `IrqWindowOpen` is swallowed as
  benign-continue (inject's own artifact, not a run-control exit). **Then add a
  regression test** with a guest that actually executes STI mid-deferral (`NOP;NOP;NOP;
  STI;...`) and assert the vector queues one step after the shadow clears — the existing
  two tests do not cover the window-*transition* path. Fix the misleading module doc
  (lines 9-11) and inline comment (lines 129-130) that currently claim the request is
  "harmless while stepping … re-checks anyway."

### Important

- [ ] **I1 — Guard or document the double-inject overwrite.**
  `crates/dh-vmm/src/inject.rs:90-98`. A second `KVM_INTERRUPT` before a VM entry returns
  `Ok` and silently overwrites the prior pending vector (verified live — no EEXIST without
  in-kernel irqchip), so `queue_interrupt`'s `rc != 0` path does not protect against a
  double-queue. Either document that callers must enter the guest between injections, or
  have `inject_at_boundary` check `ev.interrupt.injected` (already fetched in
  `injectable()`) and refuse to queue over a pending vector.

- [ ] **I2 — Reject exception-range vectors.**
  `crates/dh-vmm/src/inject.rs:84`. `KVM_INTERRUPT(14)` returns `Ok` and queues a bogus
  external IRQ through the #PF gate (verified live). Add
  `debug_assert!(vector >= 32, "external interrupt vector must be >= 32")` in
  `queue_interrupt`, or validate in `inject_at_boundary`.

### Suggestions

- [ ] **S1 — Assert the caller actually landed at `at`.**
  `crates/dh-vmm/src/inject.rs:113-120`. Add `debug_assert_eq!(counter.read()?, at.icount)`
  so a stale boundary can't produce a lying AUX record.

- [ ] **S2 — Document no-irqchip `KVM_INTERRUPT` overwrite semantics** in the module /
  `queue_interrupt` doc (pairs with I1).

- [ ] **S3 — Add a one-line comment in `injectable()`** noting NMI/SMI masking
  intentionally does not gate `KVM_INTERRUPT` (this hypervisor has no NMI/SMI source) —
  preempts the "why not check nmi.masked?" question and records the (correct) judgment.

- [ ] **S4 — Give `max_defer_steps` a documented basis** (MachineConfig field or doc
  note) before wiring into run control, so a too-small budget can't turn a slow-but-valid
  guest into a deterministic false `WindowNeverOpened`.
