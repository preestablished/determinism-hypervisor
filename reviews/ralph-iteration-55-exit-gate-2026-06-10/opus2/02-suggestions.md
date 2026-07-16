# Suggestions

## S-1. Add a "what this gate does NOT cover" subsection

Beyond the parallel-track scope note (I-1), make the document defensively
explicit about the items a careful reader would otherwise have to infer. All of
these are *correctly* deferred or out-of-scope; the issue is only that the doc
leaves them silent. Suggested bullets:

- **Cross-host-boot / cross-kernel determinism is NOT in this gate.**
  `crates/dh-verify/src/gate.rs` is explicit (lines 6–9): *"HONESTY NOTE: N runs
  in one process sample within-boot variation ... cross-host-boot and
  cross-kernel divergence is the dedicated runner's long-baseline job, not this
  gate's."* Phase 1's exit gate (criterion 1) requires "100 consecutive runs,
  zero divergence" — which is within-process, so the gate-as-run satisfies the
  *letter* of the phase doc. But state that explicitly: the long-baseline
  cross-boot corpus is the nightly drift/canary job's responsibility
  (`.github/workflows/nightly-drift.yaml`, `ci/determinism-class.lock`), deferred
  by design. Quote the honesty note so the boundary is on the record.

- **`Until::NextSdkEvent` and `Until::FrameBudget` are `NotYetWired`.**
  `crates/dh-vmm/src/runctl.rs` (lines 9–10, 40–43, 204–205, 635–640): both
  variants return `RunError::NotYetWired` pending the device-bus run loop, and
  the comment attributes them to the "M1 acceptance bead" (run loop) — i.e. they
  are *deferred wiring*, with the enum shape and error path already shipped. M3's
  acceptance (IMPLEMENTATION-PLAN M3) lists "next_sdk_event, and frame_budget
  stops" as part of `Run(until …)` semantics. Worth one line: these two `Until`
  arms are stubbed with a typed `NotYetWired` error (not silently absent) and the
  run-loop wiring lands with the device run loop. A future reader should not
  discover this by hitting the error.

- **PIO IN retirement is not separately isolated** in the counting-semantics
  attribution. `crates/dh-vmm/src/kvm.rs` (lines 297–315) documents that PIO IN
  on unmapped ports writes RAZ zeros in dispatch; the §3.1 measured rule
  ("exiting instructions retire zero") is asserted in `counting_semantics`
  (which I re-ran, 2/2 ok) covering "CPUID/PIO/MMIO/HLT retire zero, measured."
  If PIO IN is in fact covered by that "PIO ... retire zero" claim, say so; if
  the IN path is only covered transitively, note it. Minor.

## S-2. Operationally gate M4 work to reference this doc

The sequencing guard says hypervisor M4 "MUST NOT start until this gate closes,"
and the close happens in bd after merge. Suggestion (process, not blocking):
have the doc state that **M4 beads must reference `docs/phase-1-exit-gate.md`**
(or depend on dk1 in bd via `bd dep add <m4-bead> dk1`) so the guard is enforced
in the task graph, not just in a closed bead's prose. Right now the only
artifact tying M4 to this gate is the bead close; a bd dependency edge makes the
"do not start M4 until dk1 closes" rule machine-checkable in `bd ready`.

## S-3. Acknowledge 6eb as a no-op close in the handoff

Per I-3: one line ("the kvm-intel runner is registered and green for 30+ CI
runs; bead 6eb is left open only for its assignee to formally close") prevents a
future reader from misreading an open P0 runner bead as a live gate risk.

## S-4. Stale comment in `ci/determinism-class.lock`

The lock file's parse-contract comment says "for the nightly comparator, which
does not exist yet" — but `ci/check-determinism-class.sh` exists and is invoked
by `.github/workflows/nightly-drift.yaml` (drift check step). Not part of this
diff, but since the sign-off leans on the nightly drift machinery (gate row 6),
consider correcting that comment in a follow-up so the lock file does not
misrepresent its own tooling as unbuilt. Out-of-scope nit; file a bead if worth it.
