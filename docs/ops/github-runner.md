# Self-hosted GitHub runner — `kvm-intel` (Intel box)

The runner that executes every KVM-gated job (KVM integration, determinism
regression, perf gates, chaos — IMPLEMENTATION-PLAN, testing strategy). Jobs
select it with `runs-on: [self-hosted, kvm-intel]`.

| | |
|---|---|
| Runner name | `infra-control-kvm-intel` |
| Labels | `self-hosted`, `Linux`, `X64`, **`kvm-intel`** (the load-bearing one — first three are GitHub defaults shared with every other runner on this box) |
| Repo | `preestablished/determinism-hypervisor` |
| Directory | `~infra-admin/actions-runner-determinism-hypervisor` |
| Service unit | `actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service` |
| Run-as user | `infra-admin` |

This box already hosts runners for other repos, one directory per runner
(`~/actions-runner`, `~/actions-runner-verin`, …); this one follows the same
pattern. Registered 2026-06-09 with runner v2.335.1 (the runner self-updates;
its version is NOT part of the determinism class — kernel/microcode are, see
`ci/determinism-class.lock`).

## Host access the jobs rely on

- **`/dev/kvm`** — `infra-admin` is in the `kvm` group;
  `apply-host-config.sh --verify` asserts rw access when run as the runner user.
- **`perf_event_open`** — `kernel.perf_event_paranoid=1` set by the §7.4 host
  config ([host-config-intel-box.md](./host-config-intel-box.md)). `--verify`
  checks the sysctl (a proxy — it does not perform an actual
  `perf_event_open`); the real end-to-end assertion is
  `cargo run -p dh-worker --bin dh-workerd -- --preflight`, which opens
  KVM and constructs a slot VM for real (17/17 checks green as of
  2026-06-10).

## Security: public repo + privileged runner

This repo is **public** and this runner has `/dev/kvm`, a relaxed perf
paranoid level, and three other repos' runners as neighbors — fork-PR code
execution is the canonical self-hosted-runner hazard. Standing policy
(applied 2026-06-09 via `gh api`, verify in Settings → Actions → General):

- **Fork-PR approval**: `all_external_contributors` — every workflow run from
  an outside fork waits for maintainer approval, not just first-timers.
- **Default workflow token permissions**: `read` (repo contents read-only,
  cannot approve PRs).
- The CI workflow split (determinism-hypervisor-4jq) must additionally keep
  `kvm-intel` jobs off fork PRs entirely (e.g. gate on
  `github.event.pull_request.head.repo.full_name == github.repository`);
  hosted-runner jobs are the only thing a fork PR may trigger.

## Isolation from slot cores

No `CPUAffinity=` override in the service: `isolcpus=managed_irq,domain,2-5`
already keeps the runner, its jobs, and everything they spawn on the
housekeeping cores 0–1 — the scheduler never places a task on an isolated core
unless it pins itself there. KVM tests that need slot cores do exactly that,
explicitly (`sched_setaffinity`/`taskset` to 2–5), which an inherited
`CPUAffinity` mask would not prevent anyway. One rule: **nothing pins to slot
cores except guest vCPU threads and the tests that stand in for them.**

## Tool provisioning (beyond the base Rust toolchain)

Tools the milestone jobs need on this box, beyond stable Rust + the host
config. Runner jobs inherit the PATH captured in
`~/actions-runner-determinism-hypervisor/.path` at `config.sh` time — it
includes `~/go/bin`, `~/.local/bin`, and `~/.cargo/bin`, so user-local
installs are visible to jobs without touching the service unit. (If a tool
is installed to a directory NOT on that captured PATH, re-running
`config.sh` is the wrong hammer — append the directory to the `.path` file
and restart the service.)

| Tool | Needed by | Status (2026-06-12) | Install |
|---|---|---|---|
| `protoc` | tonic codegen | **Not needed** — `dh-proto` and `snapstore-client` vendor it via `protoc-bin-vendored` (proto-seam decision, iteration 60) | — |
| `grpcurl` | M6 smoke tests | ✅ v1.9.3 at `~/go/bin/grpcurl` | `go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest` |
| `cargo-fuzz` | M5 DHILOG fuzz | ✅ v0.13.2 at `~/.cargo/bin/cargo-fuzz` | `cargo install cargo-fuzz` |
| Rust nightly | M5 fuzz (cargo-fuzz requires nightly) | ✅ 1.98.0-nightly (2026-06-08) | `rustup toolchain install nightly` |
| `stress-ng` | M7 soak / chaos load | ❌ **pending — needs sudo** | `sudo apt-get install -y stress-ng` (candidate 0.17.06-1build1) |

Notes:

- **`grpcurl --version` prints `dev build <no version set>`** when installed
  via `go install` (release binaries get the version stamped via ldflags;
  go install does not). Verify the real version with
  `go version -m ~/go/bin/grpcurl | grep '^\s*mod'`.
- **Nightly drifts**: `rustup toolchain install nightly` updates in place via
  `rustup update nightly`. The fuzz lane should treat nightly breakage as
  lane-red, not gate-red — nightly is NOT part of the determinism class
  (kernel/microcode are, see `ci/determinism-class.lock`).
- **`stress-ng` is the one remaining operator step** — apt needs sudo, which
  automation on this box does not have. After installing, verify with
  `stress-ng --version` as `infra-admin`.

## Registration (already done; for rebuilds)

```bash
mkdir -p ~/actions-runner-determinism-hypervisor && cd ~/actions-runner-determinism-hypervisor
VER=2.335.1   # or: gh api repos/actions/runner/releases/latest --jq .tag_name
curl -sL -o runner.tar.gz "https://github.com/actions/runner/releases/download/v${VER}/actions-runner-linux-x64-${VER}.tar.gz"
tar xzf runner.tar.gz && rm runner.tar.gz

TOKEN=$(gh api -X POST repos/preestablished/determinism-hypervisor/actions/runners/registration-token --jq .token)
./config.sh --url https://github.com/preestablished/determinism-hypervisor \
  --token "$TOKEN" --name infra-control-kvm-intel --labels kvm-intel \
  --work _work --unattended
```

The registration token (from `gh api`, requires repo admin) is single-use and
expires after 1 hour; `svc.sh` appears only after `config.sh` has run.

## Service install / operate (root)

```bash
cd ~/actions-runner-determinism-hypervisor
sudo ./svc.sh install infra-admin   # arg = run-as user (defaults to $SUDO_USER, passed explicitly anyway)
sudo ./svc.sh start
sudo ./svc.sh status
```

Restart / logs:

```bash
sudo systemctl restart actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service
journalctl -u actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service -f
```

Health check (expect `online` once the service runs; **empty output means the
runner is not registered at all** — distinct from `offline` = registered but
service down):

```bash
gh api repos/preestablished/determinism-hypervisor/actions/runners \
  --jq '.runners[] | select(.name=="infra-control-kvm-intel") | .status'
```

## Removal

```bash
cd ~/actions-runner-determinism-hypervisor
sudo ./svc.sh stop && sudo ./svc.sh uninstall
TOKEN=$(gh api -X POST repos/preestablished/determinism-hypervisor/actions/runners/remove-token --jq .token)
./config.sh remove --token "$TOKEN"
```

## Caveats

- **One KVM job at a time**: the determinism and perf jobs assume a quiesced
  host (4 slot cores, exclusive PMU counters). A single runner instance
  already runs one job at a time — that serialization is automatic. Workflow
  `concurrency` groups add queue hygiene (collapse stale queued runs);
  note they do NOT by themselves preserve one-KVM-job-at-a-time if a
  second `kvm-intel` runner is ever added (ci.yaml's group is per-REF —
  two refs run concurrently across two runners; single-runner
  serialization is the real guarantee today). AS BUILT:
  `ci.yaml` uses a per-ref group with `cancel-in-progress: true` — CI runs
  are stateless (nothing persists from a partial run; the gate re-runs on
  the next push), so cancelling superseded runs is safe and keeps the box
  from queueing stale work. `nightly-drift.yaml` — the measurement-flavored
  workflow — uses `cancel-in-progress: false`: never kill a drift/canary
  run in flight.
- The three other runners on this box serve other repos on the housekeeping
  cores; their jobs can add noise to perf gates. Perf-gate flakiness →
  schedule the nightly when the box is quiet before touching margins.
