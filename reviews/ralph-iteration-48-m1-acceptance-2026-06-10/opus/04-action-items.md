# Action Items

### Critical

- [ ] **Fix the §7.2 CPUID mask to pin host-CPU-placement fields, and regenerate the artifact.** In `crates/dh-vmm/src/cpuid.rs::mask_in_place`, add explicit handling so the masked table is invariant across `KVM_GET_SUPPORTED_CPUID` calls on this host:
  - Leaf `0x00000001` EBX: zero bits [31:24] (Initial APIC ID); pin [23:16] (max addressable IDs) to the single-vCPU constant.
  - Leaf `0x0000000B` and `0x0000001F` (extended topology): zero EDX (x2APIC ID); prefer zeroing the whole leaf (x2APIC is already cleared in leaf-1 ECX; "no APIC at all" per ARCH §7.2).
  Then regenerate `docs/ops/cpuid-diff-infra-control.txt` (`cargo run -p dh-cli -- cpuid-diff > docs/ops/cpuid-diff-infra-control.txt`) and confirm `cargo run -p dh-cli -- cpuid-diff | diff - docs/ops/cpuid-diff-infra-control.txt` is empty across ≥5 runs.
  Evidence: same binary, 6 runs, hash flipped `4dac1b7a…` (committed) ↔ `65be8075…` (5/6 runs); root-caused to leaf-1 EBX `0x02100800`↔`0x00100800` and leaf-0xB EDX `0x00000002`↔`0x00000000`.

### Important

- [ ] **Decide what `state_hash` must cover for the run-twice claim.** Either fold `dh_vmm::hash::device_sections(&bus)` (and optionally `channel.snapshot()`, `entropy.state()`) into a final fingerprint compared between the two runs in `m1_acceptance.rs`, or narrow the "bit-identically repeatable" comment to "vCPU + guest RAM" since `run_segment` passes an empty `device_sections` slice (runctl.rs `push_final_link(seg.slot, &[], …)`).
- [ ] **Assert channel attach directly.** After the run, assert `channel.channel().is_some()` (getter exists at detchannel.rs:278) and/or `init_status == InitStatus::Ok` and `metrics.drain_failures == 0`, so "status 0 is REAL" is explicit rather than inferred from the serial + beacon assertions.

### Suggestions

- [ ] Pin the exact record count: `assert_eq!(out.log_records, 5)` (measured stable across 6 runs) instead of `>= 4`.
- [ ] Capture and compare device snapshots (`device_sections`, `channel.snapshot`, `entropy.state`) between the two runs (pre-stages M3 replay; see Important #1).
- [ ] File a debt bead: `DevCtx.boundary_rip` is 0 on the device-loop path; thread the landed `Boundary.rip` into `on_exit` before M3 replay compares rip-bearing records.
- [ ] Derive the detcall PIO window in the test from `detguest_wire::ports::{PORT_RANGE_START, PORT_RANGE_END}` instead of the hardcoded `0xD370..0xD3A0`.
- [ ] Add a CPUID-hash invariance test that fetches `masked_cpuid(&kvm)` twice (independent ioctls) and asserts equal hashes — the current self-equality test hashes one object twice and cannot catch the Critical.
