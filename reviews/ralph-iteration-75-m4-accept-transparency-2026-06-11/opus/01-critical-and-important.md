# Critical & Important Issues

## Critical

**None.** The acceptance test is correct and the milestone property it gates (H1 == H2) is sound.
The leg independence, counter-axis reasoning, and epoch-grid arithmetic all hold under scrutiny
(see 03-positive-notes.md for the verification).

---

## Important

### I1 — Module doc overstates what the H1==H2 gate proves about *device* state

**File:** `crates/dh-worker/tests/m4_transparency.rs:6-8` (and the test's final assert message at
`:271-274`)

**Problem.** The module doc says the chain catches "any instruction-count drift, **device-state
leak**, or RAM byte the restore failed to reproduce," and the final assertion message repeats
"an instruction-count, **device-state**, or RAM leak." For *this guest* that claim is only
partially true, and the gap is invisible to a future reader who trusts the comment:

1. `StateHashChain::push_final_link` is always called with `device_sections = &[]` from
   `run_segment` (`crates/dh-vmm/src/runctl.rs:318, 374, 404`). Device sections are **never**
   folded into the chain in the Phase-1 path. So H1/H2 cannot observe device state directly.
2. The chain *could* still catch a device-state leak *indirectly* — but only if the guest reads
   that device and lets the value flow into RAM or registers. The landing loop does **not**: it
   touches no pv-clock/pv-entropy/pv-pad MMIO in its loop body. Any such exit would hit the
   `on_exit` closure's `Err(BoundaryError::Exit(...))` arm (`:222-224`) and fail the run, and the
   6 green runs prove it never happens.
3. Concretely: the snapshot captures `DetEntropy::from_seed([9; 32])` and a populated
   `test_bus()`; restore reconstructs both. If `restore_snapshot` silently dropped the entropy
   PRNG word-position or mis-set `PvClock::vns_base`, **this test would still pass**, because the
   guest never consumes either. The ENTR round-trip is the IMPLEMENTATION-PLAN's *separate* M4
   "ENTR golden test (next 1024 draws bit-identical)"; that is the test that actually gates
   entropy transparency, not this one.

This isn't a correctness bug in the test — it's a precision bug in the claim. The acceptance
*does* strongly gate RAM + vCPU transparency (full-RAM walk + canonical vCPU blob at every
epoch). It does **not** gate device-state transparency for state the guest doesn't observe.

**Suggested fix.** Scope the claim in the doc and pair it with a positive device-touch
assertion, or at minimum qualify the wording. Either tighten the comment:

```rust
//! ... so any instruction-count drift or RAM/vCPU byte the restore failed
//! to reproduce shows here. Device-state transparency for state the guest
//! does NOT read (entropy word-position, pv-clock vns_base) is out of this
//! gate's reach — the chain hashes `&[]` device sections (runctl.rs) and the
//! landing loop touches no device MMIO; the ENTR golden test (M4 plan) owns
//! that axis.
```

…and likewise trim "device-state" from the `:271-274` assertion message, OR (preferred, cheap,
and it would *earn back* the original claim) add a device-reading checkpoint — e.g. after the
restored leg, downcast the restored `PvClock` and assert its `vns_base == r1.vns`, and/or pull a
draw from `outcome.entropy` and compare it to a draw from a control `DetEntropy::from_seed([9;
32])` advanced the same way. The latter is the ENTR golden test in miniature and would make the
"device-state leak shows here" wording literally true within this file. See 02-suggestions.md S1.

**Severity rationale.** Important, not Critical: nothing is *wrong*, but this is the M4
acceptance gate and its self-description is the document future agents will cite for "what M4
proved." An overstated guarantee in the canonical acceptance is the kind of thing that lets a
real device-restore regression slip past M5/M6 unnoticed.
