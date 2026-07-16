# Positive Notes

These are not throwaway compliments — each is something I verified and that a
maintainer should be glad is true.

## P-1. The "What this unblocks" handoff is real, not aspirational

Every staged-machinery claim in the sign-off's "What this unblocks" section
grep-verifies against shipped, documented code. This is exactly what an M4
implementer needs and it is rare to find a handoff this honest:

- **`vt vns_base` / `PvClock::set_vns_base`** — `crates/dh-devices/src/clock.rs:71`
  (`pub fn set_vns_base`), with `vns_base: 0` fresh-boot default (line 62) and a
  comment saying restore paths call it (line 57). There is even a test
  `vns_base_keeps_guest_time_continuous_across_restore` (line 315). Real.
- **EVTC restore takes a FRESH FaultPlan** — `crates/dh-devices/src/detchannel.rs`
  `restore` (lines 206–225) documents the §8.3 restore-order precondition and
  *"Takes a FRESH `plan`"* explicitly, calling the (EVTC, fresh-plan) pair "the
  fork path's seam." This is precisely the subtle invariant M4 must honor. Real
  and well-commented.
- **§8.1 doc-order MSR blob with normalized-TSC slot** — `crates/dh-vmm/src/hash.rs`
  (lines 11, 15, 35–38, 58): the MSR capture list is hashed in the doc's order,
  IA32_TSC is hashed in NORMALIZED (vns) form at its §8.1 position between
  TSC_AUX and SPEC_CTRL "so the M4 DHSNAP codec serializing in doc order produces
  the same preimage." The hash path was deliberately pre-aligned to the future
  M4 codec. Real and forward-designed.
- **dirty-ring caps already in `KvmCaps`** — `crates/dh-vmm/src/lib.rs:50`
  (`pub dirty_ring: bool`), populated from `KVM_CAP_DIRTY_LOG_RING_ACQ_REL`
  (`crates/dh-vmm/src/kvm.rs:112–114`), with `DIRTY_RING_ENTRIES = 65536` and the
  enable path (kvm.rs:133–138). Real.
- **`SegmentHeader` encoder fingerprint** and **`StateHashChain::from_value`** —
  `SegmentHeader` is in active use across the codebase (`dh_inputlog::dhilog`);
  `StateHashChain` is the chained-hash type used by the regression and
  m1_acceptance tests. The chain-continuation seam M4 needs exists.

## P-2. Disclosures are correct and not hidden — when they appear

The "Known refinements baked into the gate (not exceptions)" section is candid:
the §3.1 exit-instruction-retires-ZERO measured rule (reconciled against the
spec, beads 0sc/gfb), the single-step re-arm-after-MMIO-write fix
(regression-pinned), and the host-placement-invariant CPUID mask. These are the
kind of "we found this the hard way" notes that make a sign-off trustworthy.
`counting_semantics` (which I re-ran) directly exercises the
`landing_across_an_mmio_write_does_not_free_run` case — the fix is test-pinned.

## P-3. The skid artifact is well-formed and cross-doc-consistent

- The histogram is **filename-dated** (`skid-histogram-2026-06-10.txt`),
  defusing stale-dating risk — a re-run produces a new dated file rather than
  silently overwriting.
- max=79 in the archived 50k run is consistent with `README.md` lines 53–54
  ("observed maxima 39–81 across separate 50,000-sample runs (stochastic tail)").
  79 sits inside 39–81. Consistent.
- `GATE OK: max skid 79 < skid_margin/2 (4096)` — the gate has ~52x headroom;
  R1 (PMI skid) is comfortably mitigated.
- I reproduced the shape at 2,000 samples (mode 27/30/31, max 54). The dominant
  buckets match the archived run's 26/27/30/31 cluster.

## P-4. The sequencing guard is correctly invoked

The doc quotes the *exact* guard from the phase doc ("hypervisor M4 (snapshots)
MUST NOT start until this gate closes — snapshotting a nondeterministic VM
produces unfalsifiable bugs") and ties the satisfied guard to the determinism +
landing evidence. The logical chain — "M4 needs determinism proven; determinism
is proven here; therefore M4 is unblocked" — is sound. This is the one thing the
gate absolutely had to get right, and it did.

## P-5. CI / host pinning is real

`ci/branch-protection.json` lists `kvm-intel` plus both host legs as required
checks (matching gate row 6's "both host legs"); `ci/determinism-class.lock`
pins kernel/microcode; `.github/workflows/nightly-drift.yaml` wires the drift
check + 1e9-twice canary with auto-issue-filing on failure. The TSC decision
doc (`docs/decisions/tsc-alignment.md`) records the measured 932 vs 1107 ns/call
numbers and the chosen `KVM_VCPU_TSC_OFFSET` attribute with the sync-heuristic
hazard — matching gate row 7 exactly.
