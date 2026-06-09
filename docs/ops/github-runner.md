# Self-hosted GitHub runner — `kvm-intel` (Intel box)

The runner that executes every KVM-gated job (KVM integration, determinism
regression, perf gates, chaos — IMPLEMENTATION-PLAN, testing strategy). Jobs
select it with `runs-on: [self-hosted, kvm-intel]`.

| | |
|---|---|
| Runner name | `infra-control-kvm-intel` |
| Labels | `self-hosted`, `Linux`, `X64`, **`kvm-intel`** |
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

- **`/dev/kvm`** — `infra-admin` is in the `kvm` group.
- **`perf_event_open`** — `kernel.perf_event_paranoid=1` set by the §7.4 host
  config ([host-config-intel-box.md](./host-config-intel-box.md)); no file caps
  on any binary.
- Both are asserted by `bash docs/ops/apply-host-config.sh --verify` and, once
  it lands, `dh-workerd --preflight`.

## Isolation from slot cores

No `CPUAffinity=` override in the service: `isolcpus=managed_irq,domain,2-5`
already keeps the runner, its jobs, and everything they spawn on the
housekeeping cores 0–1 — the scheduler never places a task on an isolated core
unless it pins itself there. KVM tests that need slot cores do exactly that,
explicitly (`sched_setaffinity`/`taskset` to 2–5), which an inherited
`CPUAffinity` mask would not prevent anyway. One rule: **nothing pins to slot
cores except guest vCPU threads and the tests that stand in for them.**

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
sudo ./svc.sh install infra-admin
sudo ./svc.sh start
sudo ./svc.sh status
```

Restart / logs:

```bash
sudo systemctl restart actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service
journalctl -u actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service -f
```

Health check (expect `online` once the service runs):

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
  host (4 slot cores, exclusive PMU counters). Workflow jobs targeting
  `kvm-intel` must set `concurrency` so runs queue rather than overlap
  (determinism-hypervisor-4jq owns the workflow split).
- The three other runners on this box serve other repos on the housekeeping
  cores; their jobs can add noise to perf gates. Perf-gate flakiness →
  schedule the nightly when the box is quiet before touching margins.
