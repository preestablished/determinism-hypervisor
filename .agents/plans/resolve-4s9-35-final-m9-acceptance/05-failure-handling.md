# Failure Handling

The final suite is allowed to fail while investigating. It is not allowed to
be weakened to pass.

## General Rules

- Do not set `DH_M9_ALLOW_SKIP=1` for any final Linux gate.
- Do not set `DH_M7_ACCEPT_ALLOW_SKIP=1` for final Linux or nanokernel M7 gates.
- Do not remove `--ignored` from acceptance commands to make zero tests run.
- Do not reduce `DH_M7_ACCEPT_JOBS=1000` for the primary final Linux M7 gate.
- Do not replace full Linux M7 with the 100-child nightly canary.
- Do not run two KVM/M7 acceptance commands concurrently.
- Do not accept a filtered ignored-test transcript that reports `0 tests`.
- Do not run operator-shell M7 evidence while the `kvm-intel` runner service
  is able to start unrelated CI/nightly work on the same host, unless the
  whole suite itself is running inside a single reserved GitHub runner job.

If a failure is environmental, fix the environment and rerun the affected gate.
If a failure is behavioral, keep `4s9.35` open or blocked and file/update a
specific bead for the defect.

## Artifact Failures

Symptoms:

- missing `DH_M9_*` env var;
- path is not a regular file;
- initramfs contract test fails;
- artifact hash differs from the documented reference without an explanation.

Actions:

1. Re-export the documented paths from `02-reference-host-preflight.md`.
2. Confirm the files under `$HOME/.cache/dh-m9/reference-workload`.
3. Re-run `b3sum`.
4. Re-run `linux_fixture_contract`.

If the artifact bytes are wrong or stale, do not close `4s9.35`. Restore the
reviewed reference-workload artifacts or file a bead to regenerate and review
them.

## Host Class Or KVM Failures

Symptoms:

- `ci/check-determinism-class.sh` reports drift;
- `/dev/kvm` is missing or not writable;
- `dh-workerd --preflight` fails;
- `taskset -c 2-5` does not expose CPUs 2-5.

Actions:

1. Run `bash docs/ops/apply-host-config.sh --verify`.
2. Check whether the shell is running inside a restricted cpuset.
3. Move to the self-hosted runner shell or a shell with slot-core access.
4. If kernel/microcode changed, follow `docs/ops/host-config-intel-box.md`
   instead of rebaselining ad hoc.

Do not accept evidence from a non-reference host unless the relevant docs and
beads are updated through review.

## Runner Reservation Failures

If the runner service cannot be stopped for an operator-shell run, either run
the whole suite inside a single `workflow_dispatch` job on `kvm-intel` or
schedule a maintenance window. A best-effort `gh run list` check without
reservation is not enough for final M7 determinism evidence.

If the runner was stopped, make restarting it part of failure cleanup:

```bash
sudo -n systemctl start actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service
sudo -n systemctl status actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service --no-pager
```

## Workspace Or Nanokernel Failures

If `cargo test --workspace`, default `dh-cli gate`, nanokernel M5, or
nanokernel M7 fails, treat it as a regression in existing coverage. Create or
update a focused bead and leave `4s9.35` open.

Useful first probes:

```bash
cargo test --workspace -- --nocapture
cargo run -p dh-cli -- gate --runs 10
cargo test -p dh-worker --test m7_fork_verify -- --nocapture
```

Do not edit nanokernel fixtures or corpus bytes during `4s9.35` unless the
failure analysis proves a reviewed fixture rebaseline is required.

## Linux Phase 1 Failures

If Linux READY, timer, or landing/counting fails:

- Compare live artifact hashes to the final evidence docs.
- Re-run `linux_fixture_contract`.
- Check `docs/upstream-divergences.md` entries for READY, `/dev/vdb`, cmdline,
  and gate classification before changing code.
- Isolate whether the failure is before READY, at READY identity, or during
  post-READY budget/timer delivery.

File or update a bead if a deterministic contract changed. Do not mask the
failure as accepted drift.

## Linux M4/M5 Failures

For M4 transparency failures, capture:

- mid/control/restored icounts;
- state hashes;
- `reg_diffs`;
- `diff_pages`.

For M5 frame or pv-blk loopback failures, capture:

- frame tables;
- meta proof checksum;
- `blko_dirty_clusters`;
- VerifyReplay result.

For M5 corpus failures, capture:

- expected manifest line that mismatched;
- live `ready_snapshot_ref`, `end_snapshot_ref`, `dhilog_blake3`,
  `epoch_hashes_verified`, and `end_state_hash`.

If the failure is a legitimate code bug, fix it in a separate implementation
session or file a new bead. If the corpus needs rebaseline, do not regenerate
without reviewed artifact/host/hash-contract justification.

## Linux M7 Failures

Full Linux M7 is long. Preserve enough output to identify whether the failure
is one child, one slot, all children, VerifyReplay-only, or cross-slot-only.

Capture:

- job index and slot;
- child seed;
- child snapshot ref;
- child state hash;
- input log id;
- whether VerifyReplay emitted `Divergence`;
- `Done.end_state_hash` if present;
- parsed end counters and frame marks;
- meta I/O checksum.

Use reduced `DH_M7_ACCEPT_JOBS` only for debugging. The final acceptance must
return to `DH_M7_ACCEPT_JOBS=1000` and `DH_M7_CROSS_CHECKS=10`.

## Bead State On Failure

If a failure prevents closeout:

```bash
bd comment determinism-hypervisor-4s9.35 --stdin <<'EOF'
Final M9 acceptance blocked on <specific failing gate>.
Host/artifact context: <summary>.
Failure transcript: <local path or excerpt>.
Next action: <bead id or exact command>.
EOF
bd update determinism-hypervisor-4s9.35 --status blocked
```

If the failure creates new implementation work, file it:

```bash
bd create \
  --title "<short failing gate summary>" \
  --description "<why this blocks final M9 acceptance and what must be fixed>" \
  --type bug \
  --priority 0
bd dep add determinism-hypervisor-4s9.35 <new-bead-id>
```

Push Beads state before handing off, even if no code changed.
