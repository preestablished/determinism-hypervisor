# Positive notes

## P1 — the injectability predicate is exactly right, and its purity is correctly argued

`injectable()` (lines 71-80) gates on precisely the right state for `KVM_INTERRUPT`
delivery: `if_flag`, `ready_for_interrupt_injection`, no pending/injected exception, and
no interrupt shadow. I verified live that the STI shadow really does hold
`ready_for_interrupt_injection=0` for one instruction after STI (rip-pinned post-STI
boundary), so the shadow check is load-bearing, not decorative. The "pure function of
guest state → identical deferral on replay" argument is sound, and the read path
(`get_kvm_run` + `KVM_GET_VCPU_EVENTS`) is genuinely read-only — `VCPU_EVENTS` does not
perturb state, so the per-step double-ioctl is determinism-safe.

## P2 — the triple-fault delivery proof is a genuinely strong, non-racy test

`open_window_injects_and_delivers_live` proves delivery without an irqchip by the cleanest
possible signal: queue a vector into an empty IDT and observe the deterministic triple
fault (`Shutdown`). I ran the full suite 3× — it never flaked. The reason it can't be
racy is itself instructive: delivery happens at VM entry *before* any guest instruction
retires, so the fault fires before the landing loop could advance the counter; there is
no Shutdown-vs-Debug ordering window to lose. The assertion
`inj.delivered_icount == b2.icount` correctly pins delivery to the requested boundary.

## P3 — the raw `KVM_INTERRUPT` ioctl wrapping is correct and follows the established pattern

`ioctl_iow_nr!(KVM_INTERRUPT, 0xAE, 0x86, kvm_interrupt)` matches the kernel's
`KVM_INTERRUPT` definition (type 0xAE, nr 0x86, write-direction with `struct
kvm_interrupt`), the `ioctl_with_ref` call passes the struct by reference as the kernel
expects, and the `rc != 0` + `last_os_error()` error surfacing is the right shape. The
SAFETY comment is accurate. Consistent with the msr.rs raw-ioctl precedent the comment
cites.

## P4 — `delivered_icount` / `delivered_rip` semantics match the §3.4 AUX-record contract

The `Injection` struct records `requested_icount` (B), `delivered_icount` (first
injectable boundary ≥ B), and `delivered_rip`. This is exactly what §3.4 step 4 says
verification compares (`TIMER_FIRE.delivered_icount`), and the doc comment correctly
notes `delivered_rip` is a bonus diagnostic taken at queue time. The "queue time ==
boundary before delivery" reasoning is right: KVM delivers on the *next* entry before any
retirement, so the queue-time icount is the delivery icount.

## P5 — the window-request-cleanup discipline is correct (modulo C1)

Line 137 clears `request_interrupt_window = 0` on *every* exit path (the assignment is
after the loop, before `result` is returned), and the closed-window test explicitly
asserts `request_interrupt_window == 0` after the call — so even when C1 is fixed, the "no
state leaks past the call" invariant is already proven. Good RAII-style discipline for a
raw mmap'd field.

## P6 — `WindowNeverOpened` is the right kind of failure: deterministic, bounded, and loud

Capping the deferral at `max_defer_steps` and returning a typed error (rather than looping
forever or absorbing) matches the project's "fail loud, never silently absorb" philosophy
(cf. boundary.rs Overshoot). The closed-window test proves the failure replays
identically across two boots (`stepped == 250`, `counter == 10_000 + 250`), which is
exactly the determinism guarantee §3.4 promises. The verdict-ownership comment ("run
control decides whether that is a guest-contract violation") correctly keeps policy out of
the mechanism.
