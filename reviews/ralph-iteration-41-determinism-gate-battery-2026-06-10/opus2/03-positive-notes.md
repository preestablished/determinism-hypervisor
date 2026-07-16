# Positive Notes

### P-1 — The zero-divergence harness fails closed in every direction

`gate.rs::zero_divergence` (and `GateReport::passed`) treat **empty runs as a
FAIL** (`!self.fingerprints.is_empty()`) and **a run error as a gate error**
(`?`-propagated, not swallowed). Both are tested. This kills the two classic
ways a "0 of 0 diverged" green checkmark lies: a misconfigured `runs=0` and a
run closure that errors on the first iteration. The harness cannot report PASS
without having actually observed ≥1 successful, agreeing run.

### P-2 — Stops at the first divergence but keeps everything collected

The harness breaks at the first mismatch yet pushes the divergent fingerprint
before breaking, so the artifact carries `run 0..=i` — the agreeing prefix *and*
the offender. `first_divergence: Some(i)` plus `artifact()` listing every
fingerprint means a CI failure is diagnosable from the log alone, no re-run
needed. The doc comment explicitly calls this out and the test asserts the
length is `i+1`.

### P-3 — The agenda/counter consistency check is loud and early

`run_segment` re-reads the counter and rejects any `start_icount` that disagrees
with the hardware counter *before* compiling the agenda (`runctl.rs:179-188`).
The shared rig leans on this: `Rig::run_one` reads `start = counter.read()`
itself, so the asserted start can never drift from reality. A caller-asserted
start that's wrong would land every agenda point wrong — and this turns that
into an immediate, named error instead of a silent divergence.

### P-4 — Live verification reproduces bit-for-bit, including the timer chain

`dh-cli gate --runs 5` produced **identical fingerprints across all 5 runs** for
both sub-gates, and the timer sub-gate delivered the vector at
`delivered_icount = 1234567` — exactly `TIMER_AT`, confirming the §4
`icount_for_vns_target` → agenda → §3.4 deliver chain lands on the nose at a 1:1
clock. The plain and timer guests produce *different* hashes (as they must —
different code path, IDT, ISR), so the fingerprint is actually sensitive to the
injected event, not a constant.

### P-5 — The `defer` mode is a genuinely fixed window, unlike `mask`

The new `.defer_mode` uses a **bounded** `rcx=2000` loop (`timer_guest.asm:95-109`)
then `jmp .open_window` (STI), versus `.masked` which spins forever with IF=0.
That's what lets `if0_deferral` assert *delivery* (deferred to the first
post-STI boundary, `delivered > requested`, ISR observed) rather than just
`WindowNeverOpened`. The fixed iteration count is what makes the deferred
delivery icount identical run-to-run — the determinism property under test.

### P-6 — One queued-vector-at-segment-end, no leak

In `timer_determinism` the final segment leaves one vector queued in KVM
(undelivered because budget == deadline). KVM holds exactly one queued vector
and the next boot starts a fresh VM, so the slot is dropped per run — no
accumulation, no cross-run state leak. The ISR-count assertion (`FIRES-1`)
correctly accounts for it rather than papering over it.

### P-7 — The crate boundary is clean

`dh-verify` stays `#![forbid(unsafe_code)]` and **pure** — `gate.rs` is generic
over `FnMut(usize) -> Result<String, String>` and knows nothing about KVM, VMs,
or the guest. All the VM machinery lives in the *callers* (`dh-cli::gate`, the
test `common` rig). That's the right seam: the divergence logic is unit-testable
without hardware (6 host-only tests), and the same harness drives both the CLI
gate and the test suite without duplication.
