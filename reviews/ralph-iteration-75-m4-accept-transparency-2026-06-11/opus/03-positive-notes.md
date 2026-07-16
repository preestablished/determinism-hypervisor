# Positive Notes

### P1 — The tautology gap is genuinely closed: leg independence is airtight

The single biggest risk in an "H1 == H2" acceptance is that state silently flows from one leg into
the other and turns the assertion into `x == x`. It doesn't here:

- The control leg fully runs (`c1`, `c2`) and `drop(slot)` (`:207`) before the round-trip leg
  calls `boot()` again (`:210`), which constructs a **fresh** `SlotVm`, a **fresh** `InstRetired`,
  and a **fresh** `StateHashChain::new(&[7;32], &[7;32])` (`:158`).
- The only values crossing the boundary are `c1`, `c2`, `h2` — used **exclusively** in equality
  assertions, never fed back into the round-trip leg's execution.
- The pre-snapshot `assert_eq!(r1, c1, ...)` (`:215`) is a *full* `SegmentOutcome` equality
  (`SegmentOutcome` derives `PartialEq` over `reason`, `boundary`, `vns`, `state_hash`,
  `injections_delivered`, `timer_fired` — `runctl.rs:58-70`), so it proves the two legs are
  bit-identical *before* the snapshot. Without this, an H1/H2 match could mean "both legs were
  already wrong identically." With it, H1==H2 means exactly what the milestone claims.

The comment at `:251-253` calls this out explicitly ("Cold-boot determinism (the M3 property) must
hold first — otherwise any H1/H2 mismatch below would be ambiguous"). That's the right mental
model, stated in the code.

### P2 — The counter-axis reasoning is sound and machine-checked

The documented `counter: None` choice (`:255`, module doc `:10-19`) is correct, and the reasoning
is subtle enough to be worth restating as a confirmation:

- `boot()` returns one `counter`; it is **reused** across the snapshot/restore detour and never
  re-opened or reset (`restore_snapshot` with `counter: None` skips the `c.reset()` at
  `restore_engine.rs:312-315`).
- So when `run_more` runs the second segment, `counter.read()` still reads `HALF` (1e8), and
  `run_segment`'s invariant `if actual != seg.start_icount { return Err(...) }`
  (`runctl.rs:183-192`) **passes** — which is simultaneously the proof that the entire
  snapshot+destroy+restore detour retired **zero** guest instructions (`exclude_host` means the
  counter only moves on guest retirements). If the detour had leaked even one guest instruction,
  `start` (`:208`) would disagree with the agenda's expectations and this check would fire.
- Because the agenda is computed in counter space from `start_icount = 1e8`, the round-trip leg's
  epoch-hash points land at the **same absolute icounts** (1.5e8, 2e8) as the control leg — which
  is precisely why the chains can be compared at all. `outcome.cumulative_icount == HALF` (`:260`)
  confirms the restored TIME section agrees with the live counter.

### P3 — Epoch-grid arithmetic is correct

`epoch_len` defaults to `50_000_000` (`config.rs:84`, `DEFAULT_EPOCH_LEN`). `HALF = 1e8 = 2 ×
50M` and `FULL = 2e8 = 4 × 50M` both land exactly on the grid, so both legs hash at identical
icount points and the `assert_eq!(out.boundary.icount, start + more, "landed exactly")` in
`run_more` (`:228`) is satisfiable. `30_000_000` iters × `LANDING_LOOP_INSTRS_PER_ITER` (8,
`nanokernel/src/lib.rs:52`) = 2.4e8 of loop capacity, comfortably above FULL=2e8, so neither leg
reaches the guest's completion HLT — a HLT would surface as `StopReason::GuestHalted`, failing the
`assert_eq!(out.reason, StopReason::BudgetReached)` (`:227`). The comment at `:97-99` documents
exactly this budget reasoning. With the 1:1 `ClockRatio::default()` (`config.rs:130`), `vns ==
icount`, so `r2.vns == c2.vns` (`:308`) holds by the same arithmetic.

### P4 — Real store, real destroy, fresh slot — the round-trip is end-to-end

This isn't a mock. `spawn_store_blocking()` (`:83-116`) stands up an actual `snapstore-server`
over a UDS via `serve_for_tests`, and the test goes through the full
`take_snapshot` → `drop(slot)` / `drop(bus)` → `create_slot_vm` (a **new** VM/vCPU/RAM mapping,
no boot) → `restore_snapshot` path (`:255-259`). The "Fresh slot, fresh bus, no boot — everything
comes from the store" comment (`:245`) is accurate: the restored guest's entire state is
reconstructed from the DHSNAP container and the server-flattened page set, which is exactly the
property M4 needs to demonstrate.

### P5 — Placement decision is correct and well-documented

Putting the test in `crates/dh-worker/tests/` rather than `tests/determinism/` is the right call
and the module doc (`:21-24`) explains why: `tests/determinism` cannot import the engines without
violating ARCH §1's "nothing depends on dh-worker" rule, which is **CI-enforced** by
`crates/dh-worker/tests/arch_dependency_rule.rs` (it runs `cargo tree --invert dh-worker --edges
normal,build,dev` and fails on any dependent). The engines under test (`take_snapshot`,
`restore_snapshot`) live in `dh-worker`, so the acceptance must live alongside them. The dev-deps
(`kvm-ioctls = "0.24.0"` matching the workspace pin used in `hash.rs`/`runctl.rs`, `libc`, and the
`nanokernel` path dep) are correctly scoped under `[target.'cfg(target_arch = "x86_64")'.dev-
dependencies]` and gated by the `#![cfg(target_arch = "x86_64")]` at the top of the test — so they
never enter the downstream dependency graph and the whole target compiles to empty on other
arches. The Cargo.toml comment (`:25-28`) records the placement rationale in the manifest too.

### P6 — Mirrors the M3 regression pattern faithfully

The test reuses the exact shape of `tests/determinism/tests/regression.rs` (the M3 accept):
same `kvm_usable()` self-skip, same `gettid()`/counter routing, same fixed `[7; 32]` seed
material and `StateHashChain::new(&[7;32], &[7;32])`, same `Until::IcountBudget` /
`StopReason::BudgetReached` / "landed exactly" assertion discipline. A reader who knows M3 reads
M4 immediately. The `run_more` helper cleanly factors the per-segment boilerplate so the test body
reads as the two legs it is.
