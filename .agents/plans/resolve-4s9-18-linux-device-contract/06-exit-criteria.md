# Exit Criteria

`determinism-hypervisor-4s9.18` is ready to close only when all criteria below are true.

## Required Code State

- `CreateVm` accepts `BootSpec::BzImage` through the public worker API.
- BzImage worker boot uses `dh_vmm::boot::load_bzimage_and_enter`.
- `RunRequest.next_sdk_event` works through the public worker API.
- `RunResponse.sdk_event` is populated when the stop reason is `NEXT_SDK_EVENT`.
- Runtime guest-event retention still allows `StreamGuestEvents` to inspect ordering after a run.
- The state-hash preimage used by live run and replay includes deterministic lAPIC plus bus device sections.
- pv-blk remains the selected M9 block transport at `0xD000_4000`.
- No deterministic virtio-blk implementation is introduced for M9.
- Serial output is not used as a readiness condition.

## Required Test Evidence

Primary acceptance:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release pvblk_dev_vdb -- --ignored --nocapture
```

Additional evidence:

```bash
cargo test -p dh-worker service:: -- --nocapture
cargo test -p dh-worker --test m5_record_replay -- --nocapture
cargo test -p dh-worker --test snapshot_engine -- --nocapture
cargo test -p dh-worker --test replay_engine -- --nocapture
git diff --check
```

For hash-sensitive changes, run the relevant worker suite at least three consecutive times before merging or closing. The project memory says determinism-sensitive regressions have escaped single-pass test runs under light load.

## Required Behavioral Proof

The `linux_worker_api::pvblk_dev_vdb` test output must establish:

- Ready is EventKind 14 on detchannel.
- Ready occurs after CHANNEL_INIT, Hello, `LoadGame{dev_path="/dev/vdb"}`, `Start{}`, and expected region registration.
- No host-injected input lands before Ready.
- The Linux fixture reaches the selected game image through `/dev/vdb`.
- The host base image file is unchanged.
- pv-blk overlay and detchannel attachment state survive snapshot/restore and replay.
- Live run and replay produce matching state hashes.
- The fixture cannot use host entropy or host wall-clock time as a pre-Ready input.

## Session Close

Follow repository session close rules:

```bash
git status
git add <changed files>
git commit -m "Plan resolution for Linux device contract"
git pull --rebase
bd dolt push
git push
git status
```

Final `git status` must show the branch up to date with origin.
