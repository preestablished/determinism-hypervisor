# Positive Notes

### Cluster legitimacy — the boundary is honestly drawn

The three beads form a coherent unit (the M3 acceptance battery) and each meets its own text:

- **ksx** ("Phase-1 determinism gate harness: 100-run zero-divergence"): `dh-verify::gate` is
  the pure harness, `dh-cli gate` is the named command. The command's two sub-gates (plain
  landing run-to-N, then the same with a timer event at an exact icount) map **exactly** to the
  phase-1 doc Exit-gate item 1 wording. Confirmed live: PASS, identical hashes.
- **0zh** ("identical delivered icounts across 100 runs"): exact-match list compare via the
  fingerprint `"{delivered:?}"`, 100 cold boots. Met.
- **3t9** ("masked timer defers identically; delivered > requested"): the new `defer` mode
  produces a fixed IF=0 window, deadline lands masked, `delivered > requested` is asserted, ISR
  observed. Met.

### The 0zh host-armed-vs-guest-armed equivalence claim is documented, not hidden

The bead text says *the guest* arms every 1ms via its own MMIO loop; the test instead host-arms
the same cadence and **says so explicitly** — in the file header, the commit message, and bead
40q's notes. The equivalence claim is precise and correct: the converted timer goes through the
identical `timer_to_injection` (§4 ceil) → agenda merge → `inject_at_boundary` (§3.4 deliver)
chain regardless of arming source. What is genuinely *not* yet tested is the device-MMIO arming
path (the `TIMER_DEADLINE` write that drives `PvClock::armed()`), and that is correctly
deferred to 40q with a documented design hazard (the arm-mode poll loop is an exit storm under
today's debug loops). This is exactly the right way to scope around an unbuilt dependency:
deliver the determinism property now through the available path, name the gap, point at the
bead that closes it. The cluster also correctly does **not** claim the M3 "1e9 in CI"
regression item (that is bead 8n7) or the phase-1 sign-off (dk1).

### The budget==deadline / queued-vector arithmetic is self-consistent

I walked the cycle against `runctl.rs` and `agenda.rs`. When an injection icount equals the
final-stop budget they merge into one `StopPoint` (agenda test at line 242-244 proves it); the
walk lands, calls `inject_at_boundary` which **queues** the vector via `KVM_INTERRUPT` but does
not re-enter, then `point.final_stop` finishes the segment. So the last segment's vector is
queued-but-not-retired → ISR table count = FIRES-1 = 9, while the delivered-icount list is the
full 10. The leftover queued vector lives in `VCPU_EVENTS` inside the state-hash blob and is
identical every run — which is *why* the 100-run identity holds with a pending interrupt. The
empirical 100-run identity is the proof; the arithmetic backs it.

### Excellent comments at the genuinely subtle points

The one-entry-not-one-retirement comment in `runctl.rs` (the KVM-holds-one-vector overwrite
hazard) and the FIRES-1 explanation in the test are the kind of comments that save the next
reader an hour with a debugger. The inject.rs queue-vs-execute boundary is well documented.

### The harness design is clean and correctly minimal

- `dh-verify::gate` stays pure (`#![forbid(unsafe_code)]`, no VM deps) and is fully unit-tested
  for the three behaviors that matter: passes on identity, stops-and-reports at first
  divergence (with the report carrying everything collected up to and including the diverging
  run), and run-error/empty both FAIL. The "empty is not a pass" guard (`!fingerprints
  .is_empty()`) is a nice defensive touch — a zero-run gate can't silently look green.
- The `GateReport::artifact()` emits a diagnosable run-by-run dump, so a CI failure is
  triageable from logs without re-running the (expensive) gate.
- `zero_divergence` takes `FnMut(usize) -> Result<String, String>`, keeping the VM machinery
  entirely on the caller side — the same harness drives the CLI and both integration tests.

### Fingerprints capture the right state

`dh-cli`'s fingerprint includes `rip` (the prompt flagged this as desirable — present),
boundary icount, vns, the chained state hash, and the delivered icount. Each fingerprint is a
fresh cold boot (new `KvmSystem`/slot/counter), so the gate tests cold-start determinism, not
warm-state carryover. `install_kick_handler` is idempotent and re-called per run safely.

### Verification reproduced cleanly

`dh-cli gate --runs 3` produced byte-identical hashes across all three runs for both
sub-gates, timer delivered at exactly 1,234,567. Both integration tests passed at 5 runs after
a local patch (reverted; tree confirmed clean). Unit tests, clippy, and fmt all green.
