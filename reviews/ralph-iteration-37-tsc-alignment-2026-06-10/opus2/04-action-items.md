# Action items

### Critical

- [ ] **Fix the `get_tsc_offset` aliasing UB.** In `crates/dh-vmm/src/tsc.rs`, make the read buffer a
  `let mut raw = 0u64;` and derive `attr.addr` from `std::ptr::addr_of_mut!(raw) as u64` (give GET its own
  `kvm_device_attr` builder; keep `attr_for(Some(&raw))` for the read-only SET/HAS paths). Verified live:
  with this change `cargo test -p dh-vmm --release --lib tsc::` rounds `-123456789` back exactly; without
  it, release returns `0` and the test panics (`left: 0, right: -123456789`).
- [ ] **Make CI catch this class.** Add a `cargo test --release` pass to the kvm-intel lane in
  `.github/workflows/ci.yaml` (the live TSC round-trip is the canary). Today both lanes run only the debug
  profile, so this UB ships green. A release lane is cheap insurance for a hypervisor whose production
  builds are optimized.

### Important

- [ ] **Add the TSC-vs-vns drift caveat to `docs/decisions/tsc-alignment.md`.** State that
  `offset = vns − host_tsc_at_resume` pins `guest_tsc == vns` only at the resume instant; thereafter guest
  TSC drifts from vns at `(host_freq − clock_freq)` because the hardware TSC counts at the host rate. Note
  this is intended (ARCH §4 defense-4: TSC is "approximately virtual"; guests read time only via pv-clock;
  CPUID 0x15/0x16 zeroed so the guest cannot calibrate the host frequency). The formula needs **no**
  frequency conversion — it is correct under ARCH §4.1/§8.3's definition of the virtual TSC unit as vns.
- [ ] **Reconcile the decision with ARCH §8.3.** §8.3 step 3 still says restore does
  `KVM_SET_MSRS{IA32_TSC} ← vns` (the rejected mechanism). Either amend §8.3 or add a "supersedes §8.3
  mechanism" line to the decision, and explicitly instruct M4 to apply the offset attribute **instead of**
  an `IA32_TSC` entry in the restore `SET_MSRS` list (drop `IA32_TSC` from the restore write set; keep it
  on the §8.1 capture list for diagnostics only). Applying both fights and re-engages the sync heuristic.

### Suggestions

- [ ] **Re-measure the benchmark in release and correct the table** (`tsc-alignment.md:22-25`). The
  committed `1591 ns` MSR figure is not reproducible (≈1100 ns here, repeatably) and was produced in debug
  (the release test panics today). Hoisting the `Msrs` allocation does not narrow the gap — the cost is the
  ioctl, not the alloc — so the comparison is fair on that axis; just publish honest release numbers
  (median-of-N) and recompute the "4.8 ms/guest-s" line (`~1100 ns × 3000 ≈ 3.3 ms/guest-s`).
- [ ] **Delete the no-op `let _ = &mut msrs;`** at `tsc.rs:105` — dead code that misleadingly looks like an
  optimization barrier next to the GET path that has a real soundness problem.
- [ ] **Consider returning `Result<bool, KvmError>` (or errno-discriminating) from `has_tsc_offset_attr`**
  so an M4 caller can distinguish "feature absent" (`ENXIO`/`ENOENT`) from a programming/fd error. Optional;
  current behavior is acceptable given the asserting live test.
