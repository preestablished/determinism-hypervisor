# Action Items

Self-contained list. Nothing here blocks the merge; the determinism property itself is guarded
by the 0zh/3t9 integration tests, which CI runs on every push.

### Critical

_None._

### Important

- [ ] **Guard the `dh-cli gate` command in CI.** The kvm-intel lane (`cargo test --workspace`)
  runs the two integration tests but never invokes `dh-cli gate` — the named deliverable of
  bead ksx and phase-1 Exit-gate item 1. The live `run_gate` path (ELF selection, fingerprint
  assembly, exit-code semantics, `--runs` default) has no automated regression. Fix with either:
  (a) a workflow step `cargo run -p dh-cli -- gate --runs 5` asserting exit 0 (~3s), or (b) a
  `#[test]` over `run_gate` at small N in `tools/dh-cli/tests/`. File as a follow-up bead.
  _(Detail: 01-critical-and-important.md I1.)_

### Suggestions

- [ ] **Make `dh-cli gate --runs` parsing strict** (`tools/dh-cli/src/main.rs`). A
  missing/unparseable/typo'd `--runs` value, or the bare `gate N` form, silently defaults to the
  full 100-run (~32s) gate. Print usage + `exit(2)` on a malformed `--runs`. Same pattern exists
  in the `skid` arm — consider a shared `parse_count` helper (cleanup bead). _(S1.)_

- [ ] **Drop or comment the inert `"1000000000"` cmdline for the timer guest**
  (`tools/dh-cli/src/gate.rs`). `timer_guest` ignores the cmdline (no mode byte matches `'1'`,
  falls to STI default); only the landing loop reads it. Pass `b""` when `timer.is_some()` or
  add a one-line note. _(S2.)_

- [ ] **Return `Result` from `Rig::read_table`** (`tests/determinism/tests/common/mod.rs`).
  Two `read_slice(...).unwrap()` calls panic out of the gate closure on a (currently
  impossible) read failure instead of producing a diagnosable FAIL line, inconsistent with the
  rest of the `Result`-threaded rig. _(S3.)_

- [ ] **File a future bead to split heavy determinism tests to a nightly lane**
  (`.github/workflows/ci.yaml`). The +130s on every push is justified now (this is the M3
  acceptance and M4 is gated on it), but the regression family will grow (8n7, 8g1, gfb, M5
  100×). The plan already anticipates a push-vs-nightly split (IMPLEMENTATION-PLAN line 156).
  Trigger when the on-push budget crosses ~5 min; keep a fast small-N smoke on push. _(S4.)_

- [ ] **Consider consolidating the duplicated cold-boot dance** between
  `dh-cli/src/gate.rs::cold_fingerprint` and `common/mod.rs::Rig::boot` if a third caller
  appears (extract a `dh-vmm`-side helper). Currently intentional duplication (test-only rig vs
  shippable CLI); not actionable yet. _(S5.)_

- [ ] **Add one clarifying sentence** to `timer_determinism.rs` distinguishing the compared
  fingerprint (the 10-element delivered-icount list) from the ISR table count (9, queued-vs-
  retired). Prevents a false "off-by-one bug" read. _(S6.)_
