# Implementation Sequence

## 1. Verify The Existing Fixture Failure

Before changing anything, prove the starting point:

```bash
cpio -i --to-stdout etc/detguest/boot.toml < "$DH_M9_INITRAMFS"
```

If this prints the smoke manifest, preserve that output in the bead note or
implementation log.

Then run the current failing worker preflight:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE="$DH_M9_BZIMAGE" \
DH_M9_INITRAMFS="$DH_M9_INITRAMFS" \
DH_M9_BASE_IMAGE="$DH_M9_BASE_IMAGE" \
DH_M9_GAME_IMAGE="$DH_M9_GAME_IMAGE" \
DH_M9_IMAGE_CACHE="$DH_M9_IMAGE_CACHE" \
cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture
```

The expected current failure is `autostart unit must declare [unit.control]`.
If it fails differently, stop and update this plan before implementing.

## 2. Locate Or Add The Fixture Builder

Find the owner of `initramfs.cpio`:

- Check this repo for a fixture builder under `tests/`, `tools/`, `ci/`, or
  `docs/prompts`.
- Check the sibling reference-workload checkout if present.
- If no builder exists locally, add a small validation-only script in this repo
  and file or update the external fixture-builder work item.

Use concrete discovery commands:

```bash
find /home/infra-admin/git -maxdepth 4 -type f \
  \( -name 'boot.toml' -o -name '*initramfs*' -o -name '*refwork*' \) \
  | sort
rg -n 'refwork-ctl|expected_region|game_dev|/dev/vdb|FRAME_MARK|pv-blk|detguest' \
  /home/infra-admin/git/preestablished /home/infra-admin/git 2>/dev/null
```

Record the result in the bead note. If ownership is external, name the external
repo and issue. This repo must not silently synthesize a different workload
contract.

Do not hand-edit binary `initramfs.cpio` in this repo. The artifact is staged
outside the repo and must be reproducible from a builder or documented release.

## 3. Build Or Stage A Reference-Workload Initramfs

The next agent should use the actual fixture builder if available. The output
must replace the current smoke `DH_M9_INITRAMFS` in the local staging directory.

The replacement image must include:

- `/init`
- detguest agent or reference workload harness
- deterministic pv-blk `/dev/vdb` shim/driver if needed
- boot manifest from `02-fixture-contract.md`
- post-READY deterministic workload loop
- pv-pad frame emission support
- guest-driven deterministic IO path

If the Linux guest-side pv-blk `/dev/vdb` shim does not exist, implement or
stage it before writing host-side tests. Host-side tests cannot prove `/dev/vdb`
if the guest image never exposes it.

## 4. Add A Fixture Contract Probe

Add or extend a host-runnable probe that checks the staged initramfs before KVM:

- Parse `boot.toml`.
- Assert `[unit.control]` and expected regions.
- Assert the image is not the known smoke manifest.
- Optionally assert presence of required executable paths.

Preferred location if no better fixture tool exists:

- `crates/dh-worker/tests/linux_worker_api.rs` for worker preflight assertions;
- `tests/determinism/tests/common/mod.rs` or a new ignored test helper for
  direct VMM gates.

Keep the existing `assert_initramfs_boot_contract` strict.

## 5. Rebuild Linux Direct Tests Around The New Fixture

Once the fixture stays alive after READY, restore or create these direct tests:

- `tests/determinism/tests/linux_landing_counting.rs`
- `tests/determinism/tests/linux_timer_determinism.rs`

Use `tests/determinism/tests/linux_ready.rs` as the setup template.

For `linux_landing_counting`:

- Boot to READY twice from cold boot.
- Discover a post-READY execution window without consuming host input.
- Select at least 100 absolute icount targets inside that window.
- For every target, call `land_at`.
- Assert `boundary.icount == target`.
- Push a state-hash link at every target.
- Compare `(icount, rip, rcx, state_hash)` across cold boots.
- Include one target that is also used for a real scheduled injection or timer
  delivery.
- Record zero overshoots and zero skipped runs.
- Do not compare host exit counts as the determinism axis.

For `linux_timer_determinism`:

- Run 100 cold Linux cases.
- Schedule deterministic timer/IRQ delivery after READY.
- Compare delivered icount list, vector/source metadata, and final state hash.
- Assert no KVM PIT, IOAPIC, kvmclock, TSC-deadline, or in-kernel irqchip is
  created or advertised.

## 6. Rebuild Worker Tests Around The New Fixture

After direct tests prove post-READY execution, update worker tests:

- `crates/dh-worker/tests/linux_worker_api.rs`
- `crates/dh-worker/tests/m5_record_replay.rs`
- `crates/dh-worker/tests/m4_transparency.rs`
- `crates/dh-worker/tests/m5_frame_scheduling.rs`
- `crates/dh-worker/tests/m5_net_loopback.rs`
- `crates/dh-worker/tests/m7_fork_verify.rs`

Do not implement all of these in one giant patch if the repo can avoid it.
Land the fixture/probe first, then unblock and close the specific beads in
small Ralph iterations.

Minimum order:

1. `linux_worker_api` manifest/READY/StreamGuestEvents/region preflight passes
   enough to prove the fixture contract is real. Do not close `4s9.30` here.
2. `linux_landing_counting` passes and proves the post-READY boundary is safe.
3. `linux_timer_determinism` passes.
4. Full `linux_worker_api` passes through RestoreSnapshot, ReadGuestMemory,
   Fork, child run, and VerifyReplay.
5. M4/M5 frame and IO tests pass.
6. M5 corpus replay passes.
7. M7 Linux fork VerifyReplay passes.
8. Documentation/evidence beads are updated.

## 7. Own The Linux VerifyReplay Divergence

Replacing the smoke manifest is not enough to close `4s9.30`. That bead already
records a separate Linux replay divergence: final state hash differs after
replay with page diffs near `0xafb000..0xb21000`, PID-like drift, and
random-looking 16-byte state.

After the manifest preflight passes:

1. Re-run `linux_worker_api` with VerifyReplay enabled.
2. If VerifyReplay passes, record the old divergence as fixed by the new
   fixture and include artifact hashes in the bead note.
3. If VerifyReplay diverges before READY, classify it as a boot/runtime
   determinism issue and file or claim a host-side fix bead.
4. If VerifyReplay diverges after READY, classify it against the new workload
   ABI and either fix the fixture or file an external fixture-builder issue.
5. Do not close `4s9.30` until VerifyReplay is green or a superseding scope
   decision explicitly removes VerifyReplay from M9.

## 8. Preserve Nanokernel Coverage

Do not delete or weaken nanokernel tests while adding Linux coverage.

Before closing downstream preservation beads, rerun the nanokernel gates named
in `docs/ops/test-partitioning.md`, especially:

```bash
cargo test --workspace
cargo run -p dh-cli -- gate --runs 100
```
