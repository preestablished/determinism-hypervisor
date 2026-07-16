# Critical and Important findings

## C1 (Critical) — `request_interrupt_window=1` makes the deferral loop crash the instant a real guest opens its window

**File:** `crates/dh-vmm/src/inject.rs:131-133` (and the module doc, lines 9-11).

`inject_at_boundary`'s deferral loop does:

```rust
vcpu.get_kvm_run().request_interrupt_window = 1;
current = land_at(vcpu, counter, current.icount + 1, margins, on_exit)
    .map_err(InjectError::Boundary)?;
```

`land_at` (boundary.rs:162-166) routes **any** non-`Debug`, non-EINTR exit to `on_exit`:

```rust
match guard.run() {
    Ok(VcpuExit::Debug(_)) => {}
    Ok(exit) => on_exit(exit)?,   // <-- IrqWindowOpen lands here
    ...
}
```

With `request_interrupt_window=1` set, KVM exits `KVM_EXIT_IRQ_WINDOW_OPEN`
(`VcpuExit::IrqWindowOpen`, confirmed mapped in kvm-ioctls 0.24 vcpu.rs:1649) the moment
the interrupt window is open at VM entry — **before any guest instruction retires, and
instead of the single-step Debug trap**. `land_at` hands that to `on_exit`, whose only
sane responses today are "fatal" (the test `no_exits`, and any run-control handler that
doesn't special-case it). The vector is never delivered; the deferral aborts.

### Live reproduction (scratch, reverted)

Guest `NOP;NOP;NOP;STI;jmp $`, IF starts 0, single-step + `request_interrupt_window=1`,
driving the exact `inject_at_boundary` loop logic:

```
step 0: Debug            (NOP,  rip 0->1, IF=0)
step 1: Debug            (NOP,  rip 1->2, IF=0)
step 2: Debug            (NOP,  rip 2->3, IF=0)
step 3: Debug            (STI,  rip 3->4, IF=1, rfii=0  <- interrupt shadow)
step 4: land_at would route IrqWindowOpen to on_exit => FATAL
```

At step 3 the post-STI boundary has `injectable()==false` (STI shadow: `if_flag=1` but
`ready_for_interrupt_injection=0`), so the loop sets `request_interrupt_window=1` and
calls `land_at` again — and that KVM_RUN returns `IrqWindowOpen` with RIP pinned at 4,
forever. A direct probe (window already open at entry) shows the same: 12 consecutive
`IrqWindowOpen` exits, RIP never advancing.

### Why both shipped tests miss it

- `closed_window_defers_deterministically_live`: the landing loop never executes STI, so
  IF stays 0, `request_interrupt_window` never produces an exit — the loop just
  single-steps to the budget and reports `WindowNeverOpened`. The window-open path is
  never exercised.
- `open_window_injects_and_delivers_live`: IF is force-set via `set_regs` and one step
  refreshes `kvm_run`, so the *first* `injectable()` check is already true and the loop
  queues immediately — it never enters the `request_interrupt_window` + step branch at
  all.

So the entire window-*transition* path (closed → step → open) — the literal §3.4 step-4
scenario, and the behavior of essentially every real guest that uses STI to accept a
timer — is untested, and is broken.

### The doc makes it worse

Module doc lines 9-11 and the inline comment at 129-130 claim the line is "Harmless
while stepping … the stepped path re-checks anyway." That is precisely backwards: while
stepping, the line is **not** harmless — it converts the next entry's Debug trap into an
`IrqWindowOpen` exit that the stepped path never gets a chance to re-check, because
`land_at` errors out first.

### Fix (recommend)

`request_interrupt_window` is the cause and is **not load-bearing for the stepped path** —
single-step re-checks `injectable()` every retirement on its own. Two correct options:

1. **Simplest and correct for the stepped design: do not set
   `request_interrupt_window` at all in the stepped loop.** The comment's "load-bearing
   for future full-run deferral" is speculative; a full-run deferral path doesn't exist
   yet and, when it does, it will need its own `IrqWindowOpen`-aware exit handling
   anyway. Drop lines 131 (set) and 137 (clear). Verified-equivalent: without the
   request, the STI-shadow boundary single-steps once more (Debug), the shadow clears,
   `injectable()` becomes true, and the vector queues — delivery one instruction later,
   still fully deterministic.

2. **If the line is kept for the future full-run path**, `inject_at_boundary` must wrap
   `on_exit` so that `VcpuExit::IrqWindowOpen` is treated as benign-continue (it is
   inject's *own* artifact, not a guest exit run control should adjudicate). The wrapper
   should swallow `IrqWindowOpen` and delegate everything else to the caller's `on_exit`:

   ```rust
   let mut wrapped = |exit| match exit {
       VcpuExit::IrqWindowOpen => Ok(()),   // our own artifact; re-check next loop
       other => on_exit(other),
   };
   ```

   I recommend **option 1** — it removes a moving part, matches the module's stepped
   design, and the only cost is at most one extra deterministic step past an STI shadow.

This is the composition gap the prompt flagged; I confirmed it is real, reproducible, and
breaks the core use case. It is Critical because it silently defeats §3.4 for any
window-opening guest, and the doc misdirects the next maintainer.

---

## I1 (Important) — `queue_interrupt`'s `rc != 0` path is dead for the foreseeable design; double-inject silently overwrites instead of erroring

**File:** `crates/dh-vmm/src/inject.rs:90-98`.

The prompt hypothesized a double `inject_at_boundary` would hit EEXIST on the second
`KVM_INTERRUPT`. Live result on this kernel (no in-kernel irqchip):

```
first  KVM_INTERRUPT(0x30): Ok(())
second KVM_INTERRUPT(0x31) (no run between): Ok(())   <- no EEXIST
```

Without `irqchip_in_kernel`, KVM's `KVM_INTERRUPT` sets `vcpu->arch.interrupt.{nr,
injected}` directly and **overwrites** any prior pending vector — it does not return
EEXIST (that path only exists in the in-kernel-irqchip variant). So:

- The `rc != 0` error branch in `queue_interrupt` will essentially never fire for a
  double-queue; the prompt's "loud error" assumption does not hold for this design.
- More importantly, *if* run control ever calls `inject_at_boundary` twice across a
  boundary without an intervening entry (e.g. two agenda points colliding at the same
  icount), the first vector is **silently dropped** with no error — a determinism hazard
  that no current code path guards against.

`inject_at_boundary` itself queues exactly once and returns, so this is not live-broken
today. But the module's safety story rests partly on KVM rejecting a double queue, and
that story is false. Recommend either (a) document explicitly that callers must enter the
guest between two injections, or (b) have `inject_at_boundary` assert there is no already-
pending external interrupt before queueing (`KVM_GET_VCPU_EVENTS` already read in
`injectable()` exposes `ev.interrupt.injected`/`ev.interrupt.nr`; check it).

---

## I2 (Important) — no validation that `vector >= 32`; exception-range vectors are accepted and would deliver as bogus IRQs

**File:** `crates/dh-vmm/src/inject.rs:84-98`.

Live: `KVM_INTERRUPT(14)` returns `Ok` — KVM queues vector 14 as a maskable external
interrupt. Semantically this would deliver through IDT entry 14 (the #PF gate) as an
*interrupt*, not a fault, with no error code — a silent correctness landmine if a caller
ever passes an exception-range vector (e.g. a config/units bug). Determinism is preserved
(it's deterministic garbage), but §3.4 is about *external interrupt* injection; vectors
0–31 are reserved by the architecture.

Recommend a `debug_assert!(vector >= 32, ...)` in `queue_interrupt` (or a typed
`Result`/validation in `inject_at_boundary`). Cheap, and it turns a class of caller bugs
into an immediate panic in test/CI rather than a deterministic-but-wrong delivery that
only verification mode would eventually catch.
