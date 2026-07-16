# Critical & Important Findings

**None.**

Each scrutiny point from the review brief was examined and either confirmed
correct or downgraded to a suggestion / follow-up. Recording the reasoning so
the clean bill is auditable.

## 1. ioctl number — VERIFIED CORRECT (was the top concern)

`ioctl_iow_nr!(KVM_INTERRUPT, 0xAE, 0x86, kvm_interrupt)` expands to
`_IOW(0xAE, 0x86, kvm_interrupt)`. Kernel uapi `/usr/include/linux/kvm.h:1532`
defines `KVM_INTERRUPT _IOW(KVMIO, 0x86, struct kvm_interrupt)` and `KVMIO` is
`0xAE`. The `kvm_interrupt` struct (kvm.h:573) is a single `__u32 irq` — the code
populates `irq: u32::from(vector)`. Exact match on direction, type, nr, and
payload. The same expansion macro and 0xAE base are already proven by the
shipped `msr.rs` filter ioctl. **Empirical proof:** the live
`open_window_injects_and_delivers_live` test queues vector 0x30, and on the next
KVM_RUN entry the guest takes a deterministic triple fault (empty IDT) → KVM
`Shutdown` exit. A wrong ioctl number would either error (`rc != 0`) or be a
no-op leaving the guest spinning in its landing loop forever — neither happened;
the vector demonstrably reached the CPU. The ioctl number is correct, by both
header verification and live delivery.

## 2. injectable() staleness — BENIGN, by-design (downgraded)

`injectable()` reads `kvm_run` fields that are exit-time summaries. They are
valid precisely at a *landed* boundary (post-exit), and `inject_at_boundary`'s
doc-comment states the precondition explicitly: "starting from the landed
boundary `at` (the caller just landed there via §3.2)". The first `injectable()`
check in the loop therefore reads fields the immediately-preceding `land_at`
populated.

Failure mode if the precondition is violated (e.g. called on a never-run vCPU):
`kvm_run` is zeroed → `ready_for_interrupt_injection == 0` → `injectable()`
returns `false` → the engine defers via stepping. The first `land_at(+1)` then
runs the vCPU and refreshes the summaries. This is benign: a never-landed vCPU
simply costs one extra deferral step before the real state is observed, and the
result stays deterministic. Not a correctness bug. (See suggestion 02-#1 about
encoding the precondition in the type system rather than only the doc.)

## 3. ready_for_interrupt_injection semantics — CONFIRMED updated-each-exit

The concern: does KVM set `ready_for_interrupt_injection` only when
`request_interrupt_window` was set on that entry? If so, the *first*
`injectable()` check (where the request flag is still 0) could spuriously read 0.

**The live test settles this empirically.** In `open_window_injects_and_delivers_live`:
1. IF is set via `set_regs`, then `land_at(b.icount + 1)` single-steps **one**
   retirement. During that step `request_interrupt_window` was **0** (it is only
   set inside `inject_at_boundary`'s deferral loop, which has not run yet).
2. `inject_at_boundary` is then called and its **first** `injectable()` check
   returns `true` (the test asserts `delivered_icount == b2.icount`, i.e. zero
   deferral steps).

So `ready_for_interrupt_injection` read back as `1` after an exit on which
`request_interrupt_window` was never set — confirming KVM refreshes this field on
every exit regardless of the window request, exactly as the open-window path
relies on. The implementation does not depend on the request flag to observe an
already-open window. This is the central determinism guarantee and it holds.

## 4. exception/NMI blocking — correct vs §3.4; NMI note is a follow-up

`injectable()` checks `exception.pending == 0 && exception.injected == 0 &&
interrupt.shadow == 0`. §3.4 step 2 requires "no pending exception" and "no
interrupt shadow" — both covered, and checking `injected` as well as `pending`
is strictly more conservative (correct: an exception mid-injection also blocks).
NMI-pending is not checked; in the userspace-irqchip regime an external IRQ via
KVM_INTERRUPT and an NMI via KVM_NMI are independent queues, and KVM gates
KVM_INTERRUPT acceptance on `ready_for_interrupt_injection` (which already
reflects whatever the CPU's injection state allows). So the absence of an
explicit NMI-pending check is not a correctness gap for this path. Recorded as a
documentation follow-up (04 / Suggestions), not an Important finding.
