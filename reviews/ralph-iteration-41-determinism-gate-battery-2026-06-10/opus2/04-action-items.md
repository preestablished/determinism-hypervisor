# Action Items

Each item is self-contained. File paths are absolute.

### Critical

None.

### Important

- [ ] **A-1 — Fix the `TimerArm::deadline_vns` doc-vs-usage contract before M4 restore.**
  In `/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-vmm/src/runctl.rs:106-114`,
  the doc says `deadline_vns` is "segment-relative (the caller subtracts the
  segment's vns base)", but `timer_to_injection` (`runctl.rs:120-133`) and
  `ClockRatio::icount_for_vns_target` (`/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-vmm/src/vt.rs:51-55`)
  treat it as **absolute counter-space vns (origin 0)** — there is no vns-base
  subtraction anywhere; the only segment-relative op is the `.max(start_icount+1)`
  clamp. The two readings coincide only because the vns base is 0 for the whole
  boot and the clock is 1:1 (this is what `timer_determinism.rs` relies on).
  Action: (1) rewrite the `TimerArm` doc to state the actual contract —
  `deadline_vns` is the absolute pv-clock deadline in counter-space vns, origin
  0; the conversion is absolute with a `start+1` clamp. (2) Reconcile the
  `runctl.rs:116-119` conversion doc to make the origin explicit. (3) Add a note
  to the M4 restore bead: a nonzero segment vns base must be folded into an
  absolute `deadline_vns` by the caller, OR the API must grow an explicit
  `segment_vns_base` so the conversion rebases internally — pick one before M4
  freezes restore. No behavior change on this branch; this is preventing the
  first nonzero-base caller from trusting a wrong doc.

### Suggestions

- [ ] **A-2 — Add a cross-boot / statistical-honesty note for the 100-run claim.**
  No bead or doc covers this (verified via `bd list` and a grep of
  `/home/infra-admin/git/preestablished/determinism-hypervisor/.agents/docs/determinism-hypervisor/`).
  Add one sentence to the `zero_divergence` doc
  (`/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-verify/src/gate.rs:24`)
  or file a bead: "zero-divergence here is *within a single host boot* — it
  samples PMI/skid timing, scheduler interference, and cache/TLB state, but NOT
  host KASLR, microcode/MSR defaults, or P-state; cross-boot identity is the
  dedicated runner's / CI matrix's job." Prevents over-reading the green check.

- [ ] **A-3 — (optional) Switch `timer_determinism` to `budget = deadline + ε` and `count == FIRES`.**
  In `/home/infra-admin/git/preestablished/determinism-hypervisor/tests/determinism/tests/timer_determinism.rs:32,42`
  the current `budget == deadline` leaves the final vector queued, forcing the
  `count != FIRES - 1` caveat. Verified live: `budget = deadline + 1000` with
  `count == FIRES` PASSes, the ISR count becomes 10, and the delivered-icount
  list stays byte-identical. Readability win, not a correctness fix — keep as-is
  if you prefer the tighter budget; the existing comment is adequate.

- [ ] **A-4 — Avoid the `'1000000000'` cmdline / mode-letter collision risk.**
  In `/home/infra-admin/git/preestablished/determinism-hypervisor/tools/dh-cli/src/gate.rs:469,485`
  the timer guest is booted with first byte `'1'`, which falls through to STI
  today. Pass `b""` (as `timer_determinism.rs` does) or comment that the digit
  string is an intentional no-op chosen because it's not a mode letter, so a
  future digit-keyed mode doesn't silently change behavior.

- [ ] **A-5 — Consolidate `kvm_usable`/`gettid` in `regression.rs` onto `common`.**
  `/home/infra-admin/git/preestablished/determinism-hypervisor/tests/determinism/tests/regression.rs:24,36`
  duplicates probes that now live in
  `/home/infra-admin/git/preestablished/determinism-hypervisor/tests/determinism/tests/common/mod.rs:170`.
  Low priority; one definition of "is KVM usable" for the suite.

- [ ] **A-6 — Tie the `.defer_mode` iteration count to `INJECT_DEFER_BUDGET`.**
  `/home/infra-admin/git/preestablished/determinism-hypervisor/tests/nanokernel/asm/timer_guest.asm:95-109`
  (2000×6 ≈ 12k masked instr, ~7k deferral steps) sits safely inside
  `INJECT_DEFER_BUDGET = 65536`
  (`/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-vmm/src/runctl.rs:29`).
  It fails *closed* (loud `WindowNeverOpened`) if ever lengthened past the
  budget, which is fine — but a comment or shared const documenting the
  relationship would prevent a confusing future failure.

- [ ] **A-7 — Document gate fingerprints as opaque equality tokens.**
  `/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-verify/src/gate.rs:33-37`
  uses `String` fingerprints (fine for v1, collision risk nil). Add a one-line
  doc that they are compared for equality only and must not be parsed.
