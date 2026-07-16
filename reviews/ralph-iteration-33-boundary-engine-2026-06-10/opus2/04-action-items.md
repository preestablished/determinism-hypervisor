# Action items

Self-contained follow-ups, grouped by severity. Nothing here blocks merge.

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **S1 — Comment the conservative error-shadowing.** In `crates/dh-vmm/src/boundary.rs:163-169`,
  the post-loop `set_singlestep(&mut guard, false)?` can turn an `Ok(boundary)` into an
  `Err(Kvm(..))` if the disable ioctl fails. This is intentional (a vCPU stuck in single-step is
  unrecoverable, worse than a recomputable boundary). Add a one-line comment saying so, so nobody
  "fixes" it into a silent debug-state leak.

- [ ] **S2 — State the entry precondition.** `land_at` assumes the vCPU is NOT already in
  `KVM_GUESTDBG_SINGLESTEP` on entry. Add this to the doc comment (preferred over a defensive reset,
  which is mild over-engineering given the module's own paths always disable before returning).

- [ ] **S3 — Document the `on_exit` contract for HLT/device exits during a landing.** The callback
  receives `VcpuExit::Hlt` and device exits encountered mid-landing; the M3 scheduler needs to know
  that returning `Ok(())` resumes the landing and `Err(..)` aborts it. One sentence in the `land_at`
  doc. (§3.3/§3.4 composition.)

- [ ] **S4 — Follow-up bead: live REP-rule test.** Add a nanokernel program with a `REP` string op
  and a test that lands mid-REP, asserting a boundary is declared only on RIP advance. The REP path
  is currently structurally correct but unexercised by execution (landing_loop has no REP).
  Cross-references bead `determinism-hypervisor-8g1` (10,000-target torture).

- [ ] **S5 — (cosmetic) Structured KVM/Exit errors.** If `BoundaryError::Kvm`/`Exit` ever feed a
  machine-readable §9 divergence report, carry `errno`/context as fields rather than a pre-formatted
  `String`. Low priority.

### Reviewer notes for the next pass

- The 50-target multi-target smoke (this review's scratch) is a strong, cheap regression. Consider
  promoting a trimmed version (e.g. 10–20 targets) into the in-tree suite now, ahead of the full
  10,000-target bead 8g1, since it runs in ~2.5s and would catch drift early.
- All four in-tree live tests are correctly hardware-gated (`landing_rig` returns `None` and the test
  early-returns when `/dev/kvm` is unusable or perf is too strict), so CI on a paranoid>=2 / no-KVM
  runner skips cleanly rather than failing.
