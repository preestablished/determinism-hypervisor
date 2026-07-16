# Action items

All items are non-blocking. Verdict: APPROVE.

### Critical

_None._

### Important

- [ ] **I1 — Add `rep_loop` to the elf_shape static-shape gate.**
  In `tests/nanokernel/tests/elf_shape.rs`, function
  `every_guest_is_a_static_x86_64_exec_at_the_load_addr`, add:
  ```rust
  assert_guest_shape("rep_loop", rep_loop_elf());
  ```
  Rationale: every other shipped guest is shape-checked there; the new
  guest was added to build.rs/lib.rs but not to this cheap host-runnable
  gate, so a broken/oversize/PIE rep_loop link would only surface in the
  slow hardware-gated lane (or not at all if skipped).

- [ ] **I2 — Resolve the unused `REP_LOOP_INSTRS_PER_ITER` export.**
  In `tests/nanokernel/src/lib.rs` the const has zero references and is
  `pub`, so no dead-code warning will ever fire. Its sibling
  `LANDING_LOOP_INSTRS_PER_ITER` is backed by a disassembly shape test;
  this one is not, so the rep_loop body could drift to 5/7 instructions
  undetected. Pick one:
  - **Preferred:** add a `rep_loop_asm_matches_rust_constants` test
    (mirror the existing `landing_loop_asm_matches_rust_constants`) that
    disassembles rep_loop and asserts the body is exactly
    `REP_LOOP_INSTRS_PER_ITER` instructions with the REP MOVSB at the
    expected offset — this *uses* the const and pins the residue
    arithmetic the landing test relies on; or
  - drop the const (the test uses absolute targets, never iteration-
    derived ones, so it does not need it) and keep only the used
    `REP_LOOP_RCX_AT_REP_START`.

### Suggestions

- [ ] **S1 — Reuse `common::Rig`/`common::kvm_usable`** instead of the
  4th/5th copy of the boot boilerplate. Either loop `land_at` over
  `Rig::boot(...)`'s public `slot`/`counter`, or add a thin
  `Rig::land(...)` beside `run_one`. Removes ~30 duplicate lines.
- [ ] **S2 — Add a deliberate backward-landing negative assertion** at the
  end of the landing test (land N, then N-1, expect `Overshoot`) to keep
  the regression-direction contract visible here. (Already covered by
  `boundary.rs::stale_target_is_a_loud_overshoot_live`; defense-in-depth.)
- [ ] **S3 — Reconsider `PRODUCTION_PREFIX = 100`** vs runtime; ~20 proves
  the same margin-independence property far faster. Judgment call.
- [ ] **S4 — Clarify "everywhere else" RCX doc** in rep_loop.asm: the
  64/0 invariant holds *at landed boundaries* (RCX is 64..1 mid-REP, just
  never observed as a boundary). Cosmetic.
- [ ] **S5 — Comment the `rep_starts > REP_TARGETS/20` floor** with the
  true ~1/6 (≈167) expectation so 50 isn't mistaken for the model.
- [ ] **S6 — Calibrate any lane timeout to the observed ~95 s** (bead
  estimates ~71 s; full run measured ~95 s here).

### Verification performed by this reviewer (already done; for the record)

- Ran both committed tests in full: PASS (~95 s).
- Ran independent residue→rip + rcx analysis on real landed data via a
  scratch test: **confirmed instruction-start landing** on both guests;
  scratch test deleted, `git status` clean.
- `cargo clippy --workspace --all-targets`: clean on x86_64 and on
  aarch64 (env: CC=clang, CFLAGS=--target + -isystem /tmp/a64inc,
  AR=llvm-ar-18). Tree clean.
