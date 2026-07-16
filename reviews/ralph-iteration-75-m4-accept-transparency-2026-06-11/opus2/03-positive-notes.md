# Positive Notes

### P1 — The `r1 == c1` pre-assert is the right way to disambiguate a milestone failure

**File:** `crates/dh-worker/tests/m4_transparency.rs:213-215`

Asserting cold-boot determinism (the M3 property) *before* the snapshot detour means a
later H1!=H2 can only be the restore's fault, never a flaky boot. This is exactly the
"establish the baseline invariant first" discipline that makes a compound acceptance test
debuggable. The comment says as much and it is correct.

### P2 — The `drop(slot); drop(bus)` + `let _ = chain` teardown genuinely isolates the legs

**File:** `crates/dh-worker/tests/m4_transparency.rs:241-243`

Dropping `SlotVm` releases the VM fd, vCPU fd, and the memfd mmap before the fresh
`KvmSystem::open()` / `create_slot_vm` — so the restored slot is a truly independent KVM
object, not the same memslot reused. The `let _ = chain` shadow is a nice touch: it makes
"the old chain is dead, everything below comes from `outcome.chain`" enforced by the
borrow checker rather than by convention. The teardown is deliberate and the comment
explains the intent.

### P3 — `run_segment`'s `counter.read() == start_icount` re-check is correctly leveraged as a zero-guest-instruction proof

**File:** `crates/dh-vmm/src/runctl.rs:183-192` consumed by `m4_transparency.rs:208,265`

The module doc's claim that the `counter.read() == start_icount` guard inside `run_segment`
doubles as proof the snapshot+destroy+restore detour retired zero guest instructions is
**accurate**: the counter is `exclude_host` (`counter.rs:60`), the detour is all host-side
ioctls/reads, and the second `run_more` reads `start = counter.read()` = 1e8 then
`run_segment` re-reads and rejects any drift. Carrying the counter (`counter: None`) is
load-bearing for the absolute-epoch-grid alignment (`agenda.rs:160-170` keys epoch points
to absolute counter multiples), and the `out.boundary.icount == start + more` assert at
`run_more` (`:228`) confirms the second segment ran 1e8→2e8 in absolute space. The
reasoning chain is sound and the doc earns its length.

### P4 — Using the REAL in-process snapstore (not a mock) makes this a true end-to-end gate

**File:** `crates/dh-worker/tests/m4_transparency.rs:83-116,217,255`

`serve_for_tests` spins the actual server; `take_snapshot`/`restore_snapshot` go over the
blocking client and back. This is the R12 joint-testing posture
(`docs/decisions/snapstore-server-for-tests.md`) and means the gate exercises the page
flatten/resolve, manifest, and DHSNAP codec for real — a mocked store would have hidden
the most failure-prone seam.

### P5 — The placement rationale and dev-dep comment are correctly reasoned and CI-backed

**File:** `crates/dh-worker/Cargo.toml:24-31`, `m4_transparency.rs:21-24`, `tests/arch_dependency_rule.rs:9-41`

Living in `dh-worker/tests` because `arch_dependency_rule.rs` (CI-enforced) forbids
`tests/determinism` from importing the engines is the right call, the comment cites the
normative rule, and the dev-deps are correctly placed under
`[target.'cfg(target_arch = "x86_64")'.dev-dependencies]` (test-only, arch-gated, never in
the downstream graph) — matching the research file's "test-only crates belong in
dev-dependencies" rule.

### P6 — The counter-reset path the module doc defers to is genuinely covered elsewhere

**File:** `crates/dh-worker/tests/restore_engine.rs:418-435`

The module doc claims the §3.1 counter-reset-to-zero path is exercised in
`restore_engine.rs` rather than here. I verified it: that test passes `counter.as_ref()` to
`restore_snapshot` (`:425`), asserts the counter moved (`:434`), then asserts it reads 0
after restore (`:435`). The deferral is honest, not hand-waved.
