# Critical and Important Findings

## Critical

None.

## Important

None.

All 7 evidence rows survived re-execution / artifact-and-code audit. No number in
the table is unsupported by its underlying artifact. The two new files contain no
false claims. Detail per row below (this is the audit trail, not findings):

### Row 1 — Determinism gate 100/100, zero divergence — VERIFIED
- Re-ran `dh-cli gate --runs 10` (spot check). Both sub-gates PASS, output line
  `PHASE-1 DETERMINISM GATE: PASS (10 runs each)` — format matches
  `tools/dh-cli/src/cli.rs:46` exactly (`PASS ({runs} runs each)`).
- Plain sub-gate: icount 2000000, hash `482edfed…`, timer=None.
- Timer sub-gate: icount 2000000, hash `7e09ac13…`, timer delivered 1,234,567.
- The table's "icount 2,000,000; state hash 7e09ac13…; timer delivered at
  1,234,567" reproduces the TIMER sub-gate fingerprint exactly. `BUDGET=2_000_000`
  and `TIMER_AT=1_234_567` are the source constants (`gate.rs:22-23`). VERIFIED.

### Row 2 — Landing gate 10,000 exact, zero overshoots, REP boundaries — VERIFIED
- `cargo test … landing_precision`: 2/2 ok in 63–65 s.
- Source constants confirm the claim: `LANDING_TARGETS = 10_000`,
  `REP_TARGETS = 1_000`, first-boot margins 8192/256, second-boot 128, RCX
  mid-REP detector (`b.rcx == REP_LOOP_RCX_AT_REP_START || b.rcx == 0`),
  cross-boot tuple equality at margins 8192/256 vs 128. All as stated.

### Row 3 — Max skid < skid_margin/2, histogram archived — VERIFIED
- Re-ran `dh-cli skid --samples 50000`: max 71 < 4096, GATE OK line matches
  `cli.rs:69-73` format. Fresh run got 49,999 ≤ 31.
- The ARCHIVED file (committed in THIS diff) is internally consistent:
  count=50000, sum=1466744, min=26, max=79, and every cumulative Prometheus
  bucket is exact (le=27→16666, le=30→33332, le=31→49997, …, le=79→50000). The
  table's "max 79 < 4096" and "49,997/50,000 ≤ 31" are both supported by the
  archive's own numbers. VERIFIED.

### Row 4 — counting_semantics green — VERIFIED
- `cargo test … counting_semantics`: 2/2 ok. The two tests are
  `single_step_attribution_of_every_retirement_case` and
  `landing_across_an_mmio_write_does_not_free_run` (the MMIO-write trap
  regression named in the refinements). VERIFIED.

### Row 5 — M3 accepts green — VERIFIED
- `timer_determinism`: 1/1 ok, 91–95 s; source `FIRES=10`, `zero_divergence(…, 100, …)`
  — exactly "100 runs × 10 fires". Table says 95.7 s; my run 91–95 s (run-to-run
  variance, plausible).
- `if0_deferral`: 1/1 ok (`masked_window_deferral_identical_across_100_runs`).
- `regression`: 2/2 ok (`ten_million_twice…` and `one_billion…twice…`), 3.77–3.92 s
  (table says 5.5 s — see suggestion #2).
- `m1_acceptance`: 1/1 ok (`m1_device_exercise_end_to_end`). VERIFIED.

### Row 6 — CI determinism regression required-for-merge AND green — VERIFIED
- `ci/branch-protection.json` lists kvm-intel + both host legs as required.
- LIVE check via `gh api …/branches/main/protection`: all three contexts present
  and enforced — protection is not just a committed file, it is active.
- Run 27284395335 confirmed: head_branch=main, conclusion=success, and it IS the
  latest main run (`gh run list --branch main`). `nightly-drift.yaml` exists.
  VERIFIED.

### Row 7 — TSC alignment decision recorded with measured numbers — VERIFIED
- `docs/decisions/tsc-alignment.md` chooses KVM_VCPU_TSC_OFFSET device attr;
  measured table shows 932 ns (offset attr) vs 1,107 ns (MSR) at N=10,000; the
  MSR-path sync-heuristic hazard is documented (lines 11-15). Matches the row.

### Bead dk1 checklist → table mapping — COMPLETE, nothing weakened
The bead lists 7 items; each maps 1:1 to a row (gate 100/100 → R1; landing 10k +
REP → R2; max skid < margin/2 + histogram archived → R3; counting_semantics → R4;
M3 accepts incl. timer-100 + IF=0 deferral → R5; CI required-for-merge + green →
R6; TSC decision recorded → R7). "histogram archived" is satisfied by the archive
committed in this diff. "required-for-merge AND green" is satisfied by live
protection + SUCCESS run. No item is missing or silently weakened.

### §8 "what this unblocks" code claims — ALL EXIST
- `StateHashChain::from_value` — `crates/dh-vmm/src/hash.rs:86`.
- Doc-order MSR blob + normalized-TSC slot — `hash.rs:36-57, 302-326`.
- EVTC layout v1 — `crates/dh-devices/src/detchannel.rs:204` (`EVTC_VERSION = 1`),
  EVTC section + restore (`detchannel.rs:165-225`).
- SegmentHeader encoder fingerprint — `crates/dh-inputlog/src/dhilog.rs:61-75`
  (`encoder_fingerprint: u64`, bead 4ld), serialized at `dhilog.rs:291`.
- Empty-serial rule — `crates/dh-devices/src/serial.rs:122-129` (snapshot empty by
  design, restore rejects non-empty, clears pending), test at `serial.rs:218-225`.

### Refinements section — ACCURATE per merged history
- §3.1 retire-zero reconciliation: bead 0sc CLOSED with that exact close reason;
  bead gfb close reason corroborates ("CPUID/OUT/MMIO +0"). Verified.
- MMIO-write trap fix + regression: bead gfb close reason documents the engine bug
  ("MMIO-WRITE exits eat the single-step trap … boundary.rs now re-arms
  guest_debug after every handled exit, with a landing-across-the-write
  regression"). Code: `boundary.rs:169-176` re-arms after MMIO-write; test
  `landing_across_an_mmio_write_does_not_free_run` is green.
- CPUID placement invariance: `crates/dh-vmm/src/cpuid.rs:82` zeroes APIC-ID byte,
  `cpuid.rs:135-139` zeroes topology leaves; tests at `cpuid.rs:226,254`. Verified.

### Sequencing-guard framing — FAITHFUL QUOTE
`.agents/docs/phases/phase-1-deterministic-execution.md:78-79`: "Do not start
hypervisor M4 (snapshots) until the determinism gate above is green —
snapshotting a nondeterministic VM produces unfalsifiable bugs." The exit-gate
doc's framing (§ top: "hypervisor M4 (snapshots) MUST NOT start until this gate
closes") is a faithful restatement; the guard genuinely exists in the phase doc.

### Honest-sign-off sweep — CLEAN
- TODO/FIXME/XXX/HACK in `crates/*/src` + `tools/*/src`: 0.
- `#[ignore]` tests anywhere: 0.
- `unimplemented!`/`todo!` macros: 0. One `unreachable!` (`runctl.rs:386`) is a
  genuine invariant assertion, not a stub.
- Full workspace suite: all green, no skipped suites.
Nothing in Phase 1 is non-green that should have blocked sign-off.
