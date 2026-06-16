# Test partitioning: what runs where (bead b0h)

Two hardware classes run this repo's gates. Everything is part of
`cargo test --workspace`; the live legs self-skip when `/dev/kvm` is
not usable (open rw probe), so the same command is correct everywhere.

## Host-runnable (any machine — Linux/aarch64/CI hosted runners; macOS
## is EXPECTED to work via the rust-lld linker fallback in
## tests/nanokernel/build.rs but is not exercised in CI)

| What | Where | Notes |
|---|---|---|
| All pure unit tests | `cargo test --workspace` | devices, dhilog, config/hash preimages, agenda/vt math, verify harnesses |
| CoW contract over the ws4 fixtures | `cargo test -p dh-vmm --test blk_fixture` | real PvBlk + FileBase, no KVM |
| Channel interop (real detguest-host attach/drain) | `cargo test -p nanokernel --test channel_interop` | mock guest memory |
| ELF shape + asm constant pins | `cargo test -p nanokernel` | needs `nasm` (build.rs assembles the guests; cross-assembles fine on arm) |
| Drift-check script logic | `bash ci/check-determinism-class.sh` | compares against the LIVE host — only meaningful on the lab box, but parses anywhere |
| R12 joint tests vs the REAL snapshot-store | `cargo test -p determinism-tests --test store_joint` | spawns `snapstore-server` in-process on a TempDir over UDS (decision: docs/decisions/snapstore-server-for-tests.md) — needs only the `../snapshot-store` checkout, no provisioning, no KVM |
| aarch64 build/clippy | `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings` | KVM modules are `cfg(target_arch = "x86_64")`-gated. On an arm host this just works (CI's arm lane runs natively). On an x86 Linux box you need `rustup target add aarch64-unknown-linux-gnu` plus a cross C toolchain — see "aarch64 cross C toolchain on x86" below |

### aarch64 cross C toolchain on x86

The dep tree has C in it (blake3 NEON; zstd-sys via snapstore-client), so the
aarch64 cross-check needs a C compiler that can target aarch64:

- **With sudo**: `sudo apt install gcc-aarch64-linux-gnu`, then
  `CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc cargo clippy --target aarch64-unknown-linux-gnu ...`
- **Without sudo**: clang + user-extracted glibc headers work because clippy
  only *checks* — headers suffice, no cross linker needed:

  ```bash
  apt-get download libc6-dev-arm64-cross linux-libc-dev-arm64-cross
  dpkg -x libc6-dev-arm64-cross_*.deb  $HOME/.local/aarch64-cross
  dpkg -x linux-libc-dev-arm64-cross_*.deb $HOME/.local/aarch64-cross
  CC_aarch64_unknown_linux_gnu=clang \
  CFLAGS_aarch64_unknown_linux_gnu="--target=aarch64-unknown-linux-gnu -isystem $HOME/.local/aarch64-cross/usr/aarch64-linux-gnu/include" \
  AR_aarch64_unknown_linux_gnu=llvm-ar-18 \
  cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings
  ```

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
| M6 grpcurl + metrics smoke | [`docs/ops/m6-grpcurl-metrics-smoke.md`](./m6-grpcurl-metrics-smoke.md) | operator-run |
| M7 nightly fork/VerifyReplay canary | `DH_M7_ACCEPT_JOBS=100 DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture` | scheduled in `nightly-drift.yaml` |
| M7 fork/VerifyReplay acceptance | `DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture` | long |
| M7 throughput soak under housekeeping load | `DH_M7_SOAK_SECONDS=1800 ci/m7-throughput-soak.sh` | 30 min |

## Intel-box runbook (condensed)

1. **Apply §7.4 host config** (root, then reboot):
   `sudo bash docs/ops/apply-host-config.sh` — isolation cmdline, governor,
   THP, perf_event_paranoid. Details: `docs/ops/host-config-intel-box.md`.
2. **Verify** (non-root): `bash docs/ops/apply-host-config.sh --verify`
   and `cargo run -p dh-worker --bin dh-workerd -- --preflight`.
3. **Confirm the determinism class**: `bash ci/check-determinism-class.sh`
   — must report 7/7 keys ok. Drift ⇒ the re-baseline procedure in
   `host-config-intel-box.md` (it is a procedure, not an incident).
4. **Run the gates**: `cargo test --workspace` for non-ignored workspace
   tests, plus the hardware-gated/ignored rows above with their explicit
   commands (for example the M7 `-- --ignored` acceptance command).
5. CI wiring: pushes run `.github/workflows/ci.yaml` (hosted matrix +
   kvm-intel lane); nightly drift, canary, record/replay corpus, and
   scaled-down M7 100-fork VerifyReplay run via
   `.github/workflows/nightly-drift.yaml`. The kvm-intel job is a
   required merge check (see `CONTRIBUTING.md`).
