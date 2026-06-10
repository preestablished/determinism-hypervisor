# Test partitioning: what runs where (bead b0h)

Two hardware classes run this repo's gates. Everything is part of
`cargo test --workspace`; the live legs self-skip when `/dev/kvm` is
not usable (open rw probe), so the same command is correct everywhere.

## Host-runnable (any machine — macOS, aarch64, CI hosted runners)

| What | Where | Notes |
|---|---|---|
| All pure unit tests | `cargo test --workspace` | devices, dhilog, config/hash preimages, agenda/vt math, verify harnesses |
| CoW contract over the ws4 fixtures | `cargo test -p dh-vmm --test blk_fixture` | real PvBlk + FileBase, no KVM |
| Channel interop (real detguest-host attach/drain) | `cargo test -p nanokernel --test channel_interop` | mock guest memory |
| ELF shape + asm constant pins | `cargo test -p nanokernel` | needs `nasm` (build.rs assembles the guests; cross-assembles fine on arm) |
| Drift-check script logic | `bash ci/check-determinism-class.sh` | compares against the LIVE host — only meaningful on the lab box, but parses anywhere |
| aarch64 build/clippy | `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings` | KVM modules are `cfg(target_arch = "x86_64")`-gated (see CI for the cross-cc env if not on arm) |

## kvm-intel-gated (the lab box / self-hosted runner ONLY)

These self-skip elsewhere; on the box they run for real:

| What | Command | ~Time |
|---|---|---|
| M0 boot acceptance + SSE probe | `cargo test -p dh-cli --test boot_hello` | <1s |
| Skid histogram gate | `cargo test -p dh-cli --test skid_gate` (or `dh-cli skid --samples N`) | ~1s |
| M3 run-twice regression (1e9 ×2) | `cargo test -p determinism-tests --test regression` | ~4s |
| Timer determinism battery (100 runs) | `cargo test -p determinism-tests --test timer_determinism` | ~95s |
| IF=0 deferral | `cargo test -p determinism-tests --test if0_deferral` | ~32s |
| §3.1 counting empirics (R2) | `cargo test -p determinism-tests --test counting_semantics --test counting_smoke` | <1s |
| M2 landing precision (10k targets + REP torture) | `cargo test -p determinism-tests --test landing_precision` | ~71s |
| M1 device-surface acceptance | `cargo test -p determinism-tests --test m1_acceptance` | <1s |
| Phase-1 gate (one command) | `cargo run -p dh-cli -- gate [--runs N]` | ~32s at 100 runs |

## Intel-box runbook (condensed)

1. **Apply §7.4 host config** (root, then reboot):
   `sudo bash docs/ops/apply-host-config.sh` — isolation cmdline, governor,
   THP, perf_event_paranoid. Details: `docs/ops/host-config-intel-box.md`.
2. **Verify** (non-root): `bash docs/ops/apply-host-config.sh --verify`
   and `cargo run -p dh-worker --bin dh-workerd -- --preflight`.
3. **Confirm the determinism class**: `bash ci/check-determinism-class.sh`
   — must report 7/7 keys ok. Drift ⇒ the re-baseline procedure in
   `host-config-intel-box.md` (it is a procedure, not an incident).
4. **Run the gates**: `cargo test --workspace` (everything above), or
   the individual commands per table.
5. CI wiring: pushes run `.github/workflows/ci.yaml` (hosted matrix +
   kvm-intel lane); nightly drift + canary run via
   `.github/workflows/nightly-drift.yaml`. The kvm-intel job is a
   required merge check (see `CONTRIBUTING.md`).
