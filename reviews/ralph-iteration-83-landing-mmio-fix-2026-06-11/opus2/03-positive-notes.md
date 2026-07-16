# Positive notes

### P1 — Correct, revised root cause (and the honesty to revise it)

The original 4a3 hypothesis blamed MMIO-write *completion* eating TF and
proposed a PMI-period-1 backstop. The actual mechanism — TF survives
completions fine; the EMULATOR's Debug-delivery hook (firing when an emulated
MMIO instruction finishes) consumes the `guest_debug` arming — is both more
precise and more correct, and it was isolated empirically with a granular probe
walk rather than assumed. The in-code comment (boundary.rs:166-176) states the
measurement, the kernel version, the two observed overshoots (+18 probe / +74
goal-poll), and why hardware #DBs and emulator Debugs are treated identically.
This is exactly the standard this engine's other MEASURED comments set.

### P2 — Minimal, idempotent, correctly-placed fix

One line, on the `Ok(VcpuExit::Debug(_))` arm, re-asserting an idempotent
ioctl. No new state, no new control flow, no period re-arming (so it cannot
trip the perf sample-rate throttle the module header warns about). The cost is
honestly stated (~1µs/step) and bounded — a step that was free before is now a
step plus one ioctl. The error path still drops single-step (line 199-204) so
no caller observes a vCPU stuck in TF (risk R10 preserved).

### P3 — The probe guest is purpose-built and the vacuous-probe trap was caught

`mmio_stepper.asm` is a long-mode guest whose loop body is exactly the
doorbell cluster (imm dword write, 8-byte reg write, MMIO read) that
entropy_draw/pad_echo execute, in unbacked hole space with no device model —
isolating the KVM emulation/trap interaction from device side effects. The
header documents this isolation rationale. Critically, the commit body records
that the FIRST probe attempt (raw-code, real mode) was vacuous because real
mode misdecodes 64-bit encodings and can't reach the MMIO hole — the author
caught that the probe was measuring nothing and rebuilt it as a real guest.
That is the single most valuable kind of self-correction in determinism work.

### P4 — Two regressions that pin the exact failure and stress the general case

`landing_at_4096_across_mmio_clusters_is_exact_live` reproduces the precise
iteration-82 shape (target 4096, ~680 emulated MMIO exits — the exact overshoot
that filed the bug). `consecutive_landings_across_mmio_clusters_are_exact_live`
marches 120 consecutive landings at every stride 1..23 through the dense
region, asserting exact icount each time. The first locks the bug; the second
guards against a partial fix that handles some strides but not others. Both are
hardware-gated with the standard `kvm_usable()` skip.

### P5 — Other single-step callers correctly inherit the fix

`inject_at_boundary` (inject.rs:156) does its deferral walk via `land_at`, so
it is fixed transitively. The pause roll-forward path (runctl.rs:358) and the
agenda landings (runctl.rs:253) all route through the same `land_at` — all
fixed. The change is at the right chokepoint for everything except the one
sibling engine (`step_one_entry`, see I1).

### P6 — Verification rigor

Three consecutive full workspace suites (including landing_precision's
20k-landing M2 acceptance) plus both clippy targets (x86 + aarch64). For a
determinism-critical engine fix where the failure is intermittent free-running,
running the suite to convergence three times is the right bar — a one-shot
green would not have been trustworthy. The ITERS=400 headroom is comfortable:
the deepest target (4096) consumes ~228 of 400 iterations.
