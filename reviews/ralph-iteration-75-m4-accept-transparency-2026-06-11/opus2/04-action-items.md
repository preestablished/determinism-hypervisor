# Action Items

### Critical

- [ ] None.

### Important

- [ ] [crates/dh-worker/tests/m4_transparency.rs:1-8,271-274] Add a SCOPE CAVEAT line to the module doc: this gate proves RAM/vCPU/instruction transparency but NOT raw-guest-TSC transparency — the control leg's free-running guest TSC and the restored leg's `vns`-programmed `TSC_OFFSET` (vcpu_state.rs:188) are both normalized away by hash.rs's vns-in-TSC-slot (hash.rs:336-343), and the landing-loop guest never reads RDTSC, so the divergence is invisible by construction. (I1)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:270] Fix the `assert_eq!(r2.vns, c2.vns)` comment/message: it is a consistency check on the icount landing, not a guard on the restored `PvClock.vns_base` — both legs derive vns purely from `config.clock.vns_from_icount(icount)` (runctl.rs:312), so it cannot fail unless the boundary assert above already failed. Optionally strengthen it into a real `vns_base` check by downcasting the restored `PvClock` from `bus` and asserting `vns_base == r1.vns` (mirrors restore_engine.rs:235-242). (I2)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:271-274] Qualify "device-state leak" in the H1!=H2 failure message: device state is structurally excluded from the chain here (runctl passes `device_sections=&[]` at all push_final_link sites — runctl.rs:318,374,403), so the gate cannot catch a device leak. Replace with "a vCPU-state or RAM leak," or add an assertion over the restored bus's device sections / `vns_base` / entropy regs to actually cover it. (I3)

### Suggestions

- [ ] [crates/dh-worker/tests/m4_transparency.rs:158] Consider seeding H_0 with the real `cfg.config_hash().unwrap()` instead of the raw literal `[7;32]`, so the chain exercises the production machine_config_hash preimage (config.rs:237). Equality property is unaffected; this is fidelity only. (S1)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:202-206] Optionally add a third "uninterrupted 2e8" reference leg (`boot()` then one `run_more(.., FULL)`) asserting equality to `h2`, proving the 1e8 segment boundary itself is invisible to the chain, independent of the snapshot machinery. (S2)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:215] Add a half-line comment at the `r1 == c1` assert noting `SegmentOutcome`'s derived `PartialEq` includes `state_hash`, so the assert covers the full chain, not just boundary/reason. (S3)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:230] Add a one-line comment justifying `agenda_empty: true` (the producing segment ran with no injections/timer, so the boundary is quiescent). (S4)
- [ ] [crates/dh-worker/tests/m4_transparency.rs:197-200] Confirm at the pipeline level that the kvm-intel lane asserts this milestone test actually RAN (did not self-skip to green) — the `kvm_usable()` early return reports pass on any box without `/dev/kvm`. (S5)
- [ ] [crates/dh-worker/Cargo.toml:22,29,30 + crates/dh-vmm/Cargo.toml:20-22 + crates/dh-detclock/Cargo.toml:8] Promote `kvm-ioctls = "0.24.0"`, `libc = "0.2.186"`, and `vm-memory = "0.18.0"` to `[workspace.dependencies]` and switch the three manifests to `.workspace = true`; the version literals are now duplicated/triplicated with no single source of truth, while the repo already uses `.workspace = true` for every other shared dep. (Cargo hygiene)
- [ ] [crates/dh-worker/tests/{m4_transparency,restore_engine,snapshot_engine}.rs] De-duplicate `test_bus()` and `spawn_store_blocking()` (now copy-pasted across all three dh-worker test files) into `crates/dh-worker/tests/common/mod.rs` — the Rust `tests/common/mod.rs` shared-fixture pattern the determinism package already uses (tests/determinism/tests/common/mod.rs). Note `restore_engine.rs`'s copy uses a `CLOCK_BASE` const for the pv-clock base while the other two inline `0xD000_2000`; reconcile when sharing. (Maintainability)
