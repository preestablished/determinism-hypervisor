# Action Items

### Critical

- [ ] **Fix the `&u64` aliasing UB in `get_tsc_offset` (tsc.rs:77-90).** The kernel writes
  through `attr.addr` into the local `raw`, but `raw` is non-`mut` and its address came from
  a shared `&u64` with no `UnsafeCell`. This is UB in the Rust opsem model and the read-back
  (`raw as i64`) is optimizer-foldable to the init value `0` under release/LTO — the function
  can silently always return 0. Change to `let mut raw = 0u64;` and derive the address from a
  `&mut` (e.g. `core::ptr::addr_of_mut!(raw) as u64`, or `&mut raw as *mut u64 as u64`). The
  outer `ioctl_with_ref(&attr)` stays correct — only the *inner* data pointer must be
  mut-derived. `set_tsc_offset` and `has_tsc_offset_attr` are already correct (kernel reads
  only).
  **Verify:** after the fix, run `cargo test -p dh-vmm tsc --release -- --nocapture` (not just
  debug) and confirm the −123,456,789 round-trip still passes in an optimized build.

### Important

- [ ] **Encode read/write direction in the `attr_for` helper (tsc.rs:39-46).** Today it
  takes `Option<&u64>` (shared) for all callers and int-casts away the provenance, which is
  exactly how the Critical bug hid. Split into a read/none helper (SET/HAS) and a write helper
  (GET) that takes `&mut u64`, so the borrow model and the SAFETY comments agree by
  construction and a future write-direction attr cannot reintroduce the UB. (Or, if keeping a
  single `u64`-address helper, document at the GET call site that it must pass an
  `addr_of_mut!`-derived address.)

### Suggestions

- [ ] **Delete the dead `let _ = &mut msrs;` (tsc.rs:105)** and demote `let mut msrs` to
  `let msrs`. `KVM_SET_MSRS` does not write back through the buffer; the line is a leftover
  and falsely implies a kernel write-back.
- [ ] **Add a units note to the decision doc (tsc-alignment.md:230-231).** State that
  `offset = vns − host_tsc_at_resume` is well-typed only under the default
  `clock_num=clock_den=1` "deterministic 1 GHz" convention (ARCH §… / ARCHITECTURE.md:341-343),
  where guest TSC is 1 tick per vns; note CPUID leaf 0x15 is zeroed so the 1 GHz rate is
  imposed, not advertised; give the `clock_den/clock_num` scaling for the non-unity case.
- [ ] **Refresh or caveat the benchmark numbers (tsc-alignment.md:213-218).** Doc says
  986/1591; this review measured 1117/1489 (run-to-run variance, conclusion unchanged).
  Re-capture after the Critical fix or annotate as a representative single run (±~15%).
- [ ] **Note the bead file-scope deviation.** Bead 3np says `Files: …/run*`; mechanism landed
  in new `tsc.rs` (the better, more cohesive choice — M4 restore in `run.rs` will call it).
  Record the intentional deviation in the bead/commit for auditability.

### Verification performed by this review (no action needed)

- `cargo test -p dh-vmm tsc -- --nocapture` → pass; offset-attr 1117 ns, msr-write 1489 ns (N=10k)
- `cargo test --workspace` → all green
- `cargo clippy -p dh-vmm --all-targets` → clean
- `cargo fmt --check` → clean
- ioctl 0xe1/0xe2/0xe3 + `_IOW` directions → match kernel uapi (kvm.h:1519-1521)
- ARCH §4.4 / §8.3 "IA32_TSC ← vns" + 1 GHz convention → confirmed
- `KVM_CAP_VCPU_ATTRIBUTES` in `REQUIRED_RAW_CAPS` gates the attr the test asserts → confirmed (kvm.rs:78-81)
