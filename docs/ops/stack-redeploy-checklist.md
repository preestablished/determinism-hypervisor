# Intel-box stack redeploy checklist (snapstore-server + dh-workerd)

One page, sanitized. The full procedure is
`docs/ops/rom-bridge-o73-ready-snapshot.md`; this is the order of
operations an operator follows on `infra-control` to bring the stack up,
verify it, hand it off, stop it, and restart it. Never paste private
values (snapshot refs, private roots, socket paths under the private root,
ROM identity) into public notes.

Conventions: `$dist_root` = `$HOME/.cache/dh-m9/dist-<bundle version>`,
`$private_root` = the operator's private root outside every git checkout,
`$PR` = the reference-workload private root.

## 0. Preflight (stop/go)

- [ ] `.agents/plans/resolve-4s9-35-final-m9-acceptance/02-reference-host-preflight.md`
      green: host identity, `apply-host-config.sh --verify`,
      `ci/check-determinism-class.sh`, `/dev/kvm` rw, `taskset -c 2-5` ok.
- [ ] `cargo run -p dh-worker --bin dh-workerd -- --preflight` → `preflight OK`.
- [ ] `$dist_root/staging-stamp.txt` present; `DH_M9_*` exported from the
      block in `docs/ops/test-partitioning.md`.
- [ ] No other slot-core user: `pgrep -a dh-workerd`, `pgrep -a qemu`,
      `gh run list` shows no in-progress `kvm-intel` job. Kill stale
      workers/servers from earlier sessions before continuing.

## 1. Build release binaries

```bash
cargo build --release -p dh-worker --bin dh-workerd --bin dh-m9-ready-handoff
( cd ../snapshot-store && cargo build --release -p snapstore-server --bin snapstore-server )
```

## 2. Generate the READY snapshot (handoff)

- [ ] Any long-lived `snapstore-server` on the target data root is STOPPED
      (the handoff runs its own ephemeral store and refuses a live UDS).
- [ ] Run the generate command from the runbook with `--corpus-manifest`
      and `--staging-stamp`; NO `--allow-image-change` (the corpus is
      already re-baselined, so the guard must report `match`).
- [ ] Public summary: every line `yes`/`pass`, `image guard: match`.
- [ ] Modes: handoff env and snapstore config `600`, private dirs `700`.

## 3. Serve

- [ ] `test -d /run/dh || sudo install -d -m 0755 -o "$USER" /run/dh`
- [ ] Start `snapstore-server --release` on the generated config (runbook
      block); wait for its HTTP `/healthz`.
- [ ] Start `dh-workerd serve` (release) with `--uds /run/dh/grpc.sock
      --image-cache "$DH_M9_IMAGE_CACHE" --snapstore-uds "$SNAPSTORE_GRPC_UDS_PATH"
      --staging-stamp "$dist_root/staging-stamp.txt"`; pid file written.

## 4. Health

- [ ] `curl -s http://<worker http>/healthz` and `/metrics` answer.
- [ ] Worker log line contains `build_profile=release` and a non-empty
      `image_identity=`.
- [ ] `GetWorkerInfo` over the UDS (`grpcurl -unix -plaintext
      /run/dh/grpc.sock determinism.hypervisor.v1.HypervisorWorker/GetWorkerInfo`
      or `dh-cli`) reports `build_profile: release` and
      `image_identity == <bundle version>@$(b3sum $dist_root/staging-stamp.txt | cut -c1-64)`.
- [ ] Snapstore `/healthz` on its HTTP port answers; `snapstorectl` over the
      UDS lists the READY ref.
- [ ] One `CreateVm`/`DestroyVm` from `dh-cli` against the cached image
      succeeds (counts only in notes).

## 5. Record and hand off

- [ ] Write `$PR/evidence/hv-epoch-023-stack.txt` (mode 600) with every key
      listed in the runbook's "Handoff record" section.
- [ ] READY ref pinned in snapshot-store (plan C `pin-ready-root.sh`; until
      plan C lands, record the ref and keep GC disabled on this data root).
- [ ] Sanitized completion note (booleans/counts only) appended to the
      runbook evidence list or the plan's `COMPLETION.md`.

## 6. Stop

1. `kill "$(cat "$private_root/rom-bridge-o73/runtime/dh-workerd.pid")"`;
   wait until the process is gone (`ListSlots` free before killing if a
   consumer may hold leases).
2. `kill "$(cat "$private_root/rom-bridge-o73/runtime/snapstore-server.pid")"`.
3. Remove stale pid files; leave the data root and image cache alone.

## 7. Restart (after reboot or stop)

1. Section 0 preflight (short form: host identity + `--preflight`).
2. `/run/dh` exists (section 3).
3. `set -a; . "$private_root/rom-bridge-o73/handoff/bridge-real-restore-snapshot.env"; set +a`
4. Start snapstore, wait for `/healthz`; start the worker with
   `--staging-stamp`; run section 4 in full. A worker whose
   `image_identity` is empty or differs from the stamp is NOT healthy:
   stop it and fix the staging root.

## Deferred (owner: operator)

systemd units for both services follow the provisioning conventions in
`docs/ops/host-config-intel-box.md`; until they exist this checklist is
the supervision contract (`nohup setsid` + pid files).
