# Positive Notes

This is a clean, well-reasoned iteration. Highlights worth keeping.

## Determinism argument is airtight and the tests prove it

Injectability is a pure function of guest state (`ready_for_interrupt_injection`,
`if_flag`, exception pending/injected, interrupt shadow) — none of which depend
on host timing. The deferral count is therefore a deterministic function of the
guest, and `closed_window_defers_deterministically_live` proves it empirically:
two independent boots both defer **exactly 250/250** capped steps with
**bit-identical** `(stepped, counter)` tuples, and exactly one retirement per
deferral step (`10_000 + 250`). This is precisely the §3.4 guarantee ("every
replay defers by the identical number of instructions").

## The open-window delivery proof is elegant and rigorous

Rather than trusting that KVM_INTERRUPT "probably" delivered, the test forces a
*deterministic observable*: an empty IDT means the queued vector triggers a
triple fault on the very next entry, surfacing as a `Shutdown` exit — an event
that can only happen if the vector actually reached the CPU (the landing loop
would otherwise spin forever and never exit). This simultaneously proves: (a)
the ioctl number is right, (b) the vector queues, and (c) it delivers before any
guest instruction retires. Excellent test design.

## The request_interrupt_window discipline is exactly right

`request_interrupt_window` is set inside the deferral loop and **unconditionally
cleared at the single exit point** (after the `loop`, before returning) — so it
is cleared on the inject-immediately path (where it was never set: harmless), on
the success path, on `WindowNeverOpened`, and on every `Err` propagation. The
test explicitly asserts `request_interrupt_window == 0` after the call: **no
window-request leak**. The comment correctly notes the flag is "harmless while
stepping; load-bearing for future full-run deferral", which is the right
forward-looking design — the stepped path re-checks injectability anyway, so the
flag is purely there for the eventual full-run exit-on-window-open path.

## Faithful, conservative mapping to §3.4

The module maps one-to-one onto §3.4 steps 1–4, the doc-comment cites the
section, and the injectability check is *stricter* than the spec (it also
rejects `exception.injected != 0`, catching mid-injection state the spec's "no
pending exception" wording alone would miss). The `Injection` record cleanly
separates `requested_icount` (B) from `delivered_icount` (first injectable
boundary ≥ B), matching the AUX `TIMER_FIRE.delivered_icount` field that
`dh-inputlog/src/dhilog.rs:210` already defines for verification.

## Correct unsafe hygiene and ioctl precedent reuse

The single `unsafe` block carries an accurate SAFETY comment (valid fd, kernel
copies the struct), is scoped to the ioctl call, and reuses the exact
`ioctl_iow_nr!` + `ioctl_with_ref` + `#[allow(unsafe_code)]` pattern already
shipped and reviewed in `msr.rs`. No new unsafe surface beyond the one
unavoidable raw ioctl.

## land_at error-path robustness confirmed under triple fault

The triple-fault test exercises a subtle path: `land_at` was stepping
(`inj.delivered_icount + 100` with the guest exiting almost immediately via
Shutdown), so its `set_singlestep(false)` drop runs on a vCPU that just
triple-faulted — and `KVM_SET_GUEST_DEBUG` still succeeds post-Shutdown (the
test passes). Good confirmation that the boundary engine's "always drop
single-step, even on error paths" invariant holds even in degenerate guest
states.

## Quality gates green

60/60 tests pass (including live KVM/perf tests on this rw `/dev/kvm` +
`paranoid=1` host), clippy is clean with zero warnings, and the change is
self-contained (one new module + one export line).
