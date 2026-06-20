# Implementation Sequence

## Phase 1: Claim And Baseline

1. Run `bd show 4s9.27` and update it to `in_progress` if Beads allows moving it out of blocked status now that its dependencies are closed.
2. Confirm the worktree is clean with `git status --short --branch`.
3. Confirm this host still has `/dev/kvm` read/write access.
4. Run the existing guard list command:

   ```bash
   cargo test -p dh-worker --test m5_record_replay linux -- --ignored --list
   ```

5. Run the existing guard with `DH_M9_ALLOW_SKIP=0` once and confirm it fails for the documented reason before replacement.

## Phase 2: Add Linux Corpus Metadata Helpers

In `crates/dh-worker/tests/m5_record_replay.rs`:

1. Add a Linux corpus fixture directory constant:

   ```rust
   const M9_LINUX_CORPUS_DIR: &str = concat!(
       env!("CARGO_MANIFEST_DIR"),
       "/tests/fixtures/record_replay_corpus/m9_linux_post_ready"
   );
   ```

2. Add `M9_LINUX_CORPUS_EXPECTED: &str = "expected.txt"`.
3. Reuse the existing `expected_map`, `expected_value`, `expected_u64`, `blake3_hex`, `hex`, and `parse_hex32` helpers where possible.
4. Extend key validation with a separate Linux expected key set. Do not overload the nanokernel key set if that makes the fixture ambiguous.
5. Add a helper to fetch stored input-log payload by ID. Either:
   - move a generic helper into `crates/dh-worker/tests/common/mod.rs`, or
   - localize it in `m5_record_replay.rs`.

Use the existing worker test helper shape directly:

```rust
fn input_log_payload(
    store: &snapstore_client::blocking::SnapstoreClient,
    input_log_id: &[u8],
) -> TestResult<Vec<u8>> {
    let id: [u8; 32] = input_log_id
        .try_into()
        .map_err(|_| "input log id must be 32 bytes".to_string())?;
    let container = store
        .get_input_log(snapstore_types::LogId::from_bytes(id))
        .map_err(|e| format!("get_input_log: {e}"))?;
    let decoded = snapstore_manifest::input_log::InputLogContainer::decode(&container)
        .map_err(|e| format!("input log container decode: {e}"))?;
    Ok(decoded.payload().to_vec())
}
```

This helper pattern already exists in `m5_frame_scheduling.rs` and `linux_worker_api.rs`. The Linux M5 corpus test must fetch and parse the stored DHILOG; a `VerifyReplay`-only check is not enough for `4s9.27`.

## Phase 3: Add VerifyReplay Evidence Helper

Add a helper that is stricter than `common::verify_replay_done`:

```rust
struct VerifyReplayEvidence {
    done: proto::VerifyDone,
    epoch_ok_count: usize,
}
```

Behavior:

- calls `svc.verify_replay` with `base = ready.ready_snapshot_ref`;
- uses `Log::InputLogId(post_snapshot.input_log_id.clone())`;
- sets `bisect_on_divergence = Some(false)`;
- counts `EpochOk`;
- returns `Done`;
- errors on `Divergence`, empty progress, stream error, or stream ending before `Done`.

If this helper is generally useful, place it in `common/mod.rs` beside `verify_replay_done`; otherwise keep it local to avoid widening shared API.

## Phase 4: Replace The Linux Guard

Replace:

```rust
linux_m5_record_replay_requires_real_linux_corpus
```

with:

```rust
#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_m5_record_replay_post_ready_corpus_reverifies() -> TestResult<()>
```

Test flow:

1. Call `common::m9_linux_ready_snapshot("m5_record_replay::linux_m5_record_replay_post_ready_corpus", 2)?`.
2. If it returns `None`, return `Ok(())`; this only happens when `DH_M9_ALLOW_SKIP=1`, which is not accepted for final evidence.
3. Run the READY VM for the selected frame budget:

   ```rust
   proto::run_request::Until::FrameBudget(M9_LINUX_CORPUS_FRAMES)
   ```

4. Assert `RunResponse.reason == BudgetReached`.
5. Assert `RunResponse.frames_elapsed == M9_LINUX_CORPUS_FRAMES`.
6. Read the `meta` region proof at offset 32 like `m5_net_loopback` does, or collect an equivalent guest-visible post-READY proof. The normal path should require the existing `PVBLKIO1` checksum so the segment proves deterministic guest work beyond frame counting.
7. Take a snapshot with `seal_input_log = Some(true)`.
8. Assert the snapshot has a state hash and an input log ID.
9. Fetch and parse the stored DHILOG with `input_log_payload` and `LogReader::parse`.
10. Run `VerifyReplay` and assert nonzero epoch progress.
11. Destroy the VM lease.
12. Compare live evidence to `expected.txt`.

## Phase 5: Expected Manifest

Add:

```text
crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/README.md
crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt
```

The first implementation can generate `expected.txt` by printing a proposed manifest under `DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1`, then writing it manually after review. Avoid ad hoc shell redirection in the final implementation; use `fs::write` in the ignored regeneration test if regeneration remains part of the code.

Recommended contents:

```text
name=m9_linux_post_ready
host_id=infra-control
determinism_class_lock_blake3=<hash>
bzimage_blake3=<hash>
initramfs_blake3=<hash>
base_image_blake3=<hash>
game_image_blake3=<hash>
run_until=frame_budget:<n>
hard_icount_cap=<n>
machine_config_hash=<hash>
dhilog_blake3=<hash>
dhilog_records=<n>
records_applied=<n>
epoch_hashes_verified=<n>
end_icount=<n>
end_vns=<n>
end_state_hash=<hex32>
frame_counter=<n>
meta_pvblk_checksum=<hex or decimal>
epoch_<i>=<icount>:<hex32>
```

Make key names stable and assert the exact key set.

## Phase 6: Corpus Regeneration Test

Add an ignored regeneration test only if it materially helps future maintenance:

```rust
#[test]
#[ignore = "explicit re-baseline only: set DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1"]
fn regenerate_m9_rr_corpus_manifest_for_reference_host()
```

This test should:

- require `DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1`;
- run the same Linux segment;
- compute the expected manifest text;
- write only small metadata files unless the full-corpus storage mode was explicitly accepted.
- avoid `linux` in its function name so the required `linux` acceptance filter does not select a regeneration-only test.

Do not make the normal acceptance test rewrite fixtures.

## Phase 7: Clean Up Blocked Bead State

When the Linux corpus gate passes:

1. Close or update `4s9.27` with exact evidence.
2. Run `bd ready` to see which downstream bead unblocks next.
3. Do not implement `4s9.29` in this branch unless explicitly asked; leave it ready for the next agent.
