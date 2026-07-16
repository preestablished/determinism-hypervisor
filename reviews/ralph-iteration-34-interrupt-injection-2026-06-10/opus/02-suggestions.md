# Suggestions (non-blocking)

All low-risk. None block the merge.

## 1. Reject exception-vector range in `queue_interrupt` (vector < 32)

`queue_interrupt(vcpu, vector)` accepts any `u8` and forwards it as the IRQ
number. External-interrupt vectors are `32..=255`; `0..32` are CPU
exception/reserved vectors and should never arrive via KVM_INTERRUPT. The tests
use `0x30` (48), correctly in range. A guard such as

```rust
if vector < 32 {
    return Err(InjectError::Kvm(format!(
        "vector {vector} is in the exception range (<32)"
    )));
}
```

turns a programming mistake (passing an exception number where an external
vector is expected) into a loud, deterministic error at queue time rather than a
mis-delivered or kernel-rejected interrupt. Self-contained, no behavior change
for valid callers. (Note: KVM itself may accept low vectors via KVM_INTERRUPT in
the no-irqchip regime, so this is a contract guard, not a correctness fix.)

## 2. Encode the "freshly landed" precondition in the API

`inject_at_boundary` documents that `at` must be a boundary the caller *just*
landed, because `injectable()`'s first read trusts the current `kvm_run`
exit-time summaries (see 01-#2). Today this is doc-only. Consider either:
- accepting the `Boundary` *and* asserting `counter.read()? == at.icount` at
  entry (cheap, turns a stale `at` into a loud error like `land_at`'s overshoot
  guard does), or
- a brief doc note that `inject_at_boundary` must immediately follow the
  `land_at(B)` that produced `at`, with nothing run in between.

The second is enough; the first is nicer because it is self-checking and matches
the project's "deterministic and loud" stance for stale inputs (cf.
`BoundaryError::Overshoot`).

## 3. In-loop single-stepping optimization (defer until profiling demands)

Each deferral step is a fresh `land_at(current + 1)`, which enables and disables
KVM_GUESTDBG single-step per step (2 `KVM_SET_GUEST_DEBUG` ioctls/step plus the
counter park). For the tested 250-step deferral this is fine (suite runs in
0.67 s). A guest that defers for, say, hundreds of thousands of instructions
would pay that churn per retirement. **Do not act on this now** — correctness is
unaffected and the cost is invisible at current scales. If profiling later shows
deferral as a hotspot, an inject-local stepping loop that keeps single-step
armed across re-checks (re-reading the counter each `Debug` exit, like
`land_at`'s near path) would amortize the ioctls. File a perf bead only if/when
measurements justify it.

## 4. Document the NMI / KVM_INTERRUPT independence

`injectable()` deliberately does not inspect NMI-pending state (see 01-#4). A
one-line comment on `injectable()` noting that NMIs use a separate queue
(KVM_NMI) and do not gate KVM_INTERRUPT in the userspace-irqchip regime would
pre-empt the exact question this review raised, and would document the
intentional scope of the "no pending exception" check vs §3.4.
