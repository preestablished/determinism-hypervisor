# Linux Log And Replay Contract

## Linux Child Workload

Each Linux child is forked from the M9 READY snapshot returned by the M9 helper. The child workload is a deterministic post-READY frame-budget run:

```rust
RunRequest {
    lease: Some(child_lease),
    until: Some(Until::FrameBudget(M9_LINUX_CHILD_FRAMES)),
    hard_icount_cap: M9_LINUX_CHILD_HARD_CAP,
    capture: None,
}
```

Recommended constants:

```rust
const M9_LINUX_CHILD_FRAMES: u32 = 5;
const M9_LINUX_CHILD_HARD_CAP: u64 = 5_000_000;
const M9_LINUX_CHILD_EPOCH_LEN: u64 = 745_000;
```

Rationale:

- these constants are already proven by the Linux M5 post-READY corpus;
- five frames is enough to emit frame marks and reach at least one epoch hash with `epoch_len = 745000`;
- the hard cap catches stalled Linux children without hanging the full 1000-child run indefinitely.

## Required Live Run Checks

For every Linux child:

- `RunResponse.reason == BudgetReached`
- `RunResponse.frames_elapsed == M9_LINUX_CHILD_FRAMES`
- `RunResponse.icount > root_cumulative_icount`
- `RunResponse.vns >= root_cumulative_vns`
- `segment_end_icount = RunResponse.icount - root_cumulative_icount`
- `segment_end_vns = RunResponse.vns - root_cumulative_vns`
- `segment_end_icount > 0`
- `segment_end_icount <= M9_LINUX_CHILD_HARD_CAP`
- child snapshot has a snapshot ref
- child snapshot has a state hash
- child snapshot has a 32-byte input log id
- child snapshot `frame_counter == ready_frame_counter + M9_LINUX_CHILD_FRAMES`

Use absolute frame counters for frame assertions, because `FRAME_MARK` records store absolute frame counter values.

## Required DHILOG Checks

Parse every Linux child log with `LogReader::parse`.

Header and lineage checks:

- `header.base_snapshot_id == ready_snapshot_ref`
- `header.end_snapshot_id == child.snapshot.hash`
- `header.end_state_hash == child.state_hash`
- `header.machine_config_hash == ready.config_hash`
- `header.end_icount == child.segment_end_icount`
- `header.end_vns == child.segment_end_vns`
- `header.has_epoch_hashes() == true`

Auxiliary record checks:

- at least one `RecordBody::EpochHash` is present;
- exactly `M9_LINUX_CHILD_FRAMES` `RecordBody::FrameMark` records are present;
- frame mark indices exactly equal `ready_frame_counter + 1..=ready_frame_counter + M9_LINUX_CHILD_FRAMES`;
- frame mark icounts are strictly increasing;
- the final frame mark's `frame_index` equals the child snapshot `frame_counter`.

Canonical record checks:

- Linux logs may contain no canonical input records for this workload, or may contain deterministic canonical records from devices such as entropy, net, or detchannel depending on the guest workload.
- Do not require `PadSet`.
- Do not reject known non-pad canonical records solely because they are not used by nanokernel.
- Do reject unknown canonical records. `LogReader::parse` already rejects unknown canonical kinds, so this is mostly enforced by successful parse.

End record checks:

- `reader.end().1 == header.end_state_hash`
- `Lineage::end_identity()` equals the child snapshot and child state hash
- `Lineage::end_identity().end_icount == header.end_icount`

Implement this with a concrete parsed summary so the validation code is easy to audit:

```rust
struct ParsedChildLog {
    dhilog_blake3: String,
    base_snapshot_id: [u8; 32],
    end_snapshot_id: [u8; 32],
    machine_config_hash: [u8; 32],
    record_count: u64,
    canonical_count: u64,
    end_icount: u64,
    end_vns: u64,
    end_state_hash: [u8; 32],
    epoch_hashes: Vec<(u64, u64, [u8; 32])>,
    frame_marks: Vec<(u64, u32)>,
}
```

The frame-mark extraction should follow the pattern in `crates/dh-worker/tests/m5_frame_scheduling.rs`: collect `(rec.icount(), frame_index)`, compare frames to an expected table, and require strictly increasing icounts.

## Optional Meta IO Proof

The Linux M5 corpus validates a `PVBLKIO1` proof in the `meta` region at:

```rust
const M9_LINUX_META_IO_MAGIC_OFF: u64 = 32;
const M9_LINUX_META_IO_PROOF_LEN: u64 = 24;
```

If the code is still local in `m5_record_replay.rs`, either move the parser to `common/mod.rs` or duplicate the small parser in `m7_fork_verify.rs`.

For M7 Linux, meta proof validation is useful because it proves the forked Linux child executed the post-READY reference workload, not only an empty frame loop. Preferred behavior:

- read the proof before taking or immediately before destroying each child;
- require magic `PVBLKIO1`;
- require nonzero checksum;
- store checksum in `ChildRecord`;
- include checksum equality in same-seed cross-slot comparisons.

If per-child memory reads make the 1000-child acceptance unacceptably slow, validate the meta proof for the first child in each batch and for every cross-slot child, then document the runtime tradeoff in the bead comment. Do not remove frame mark, epoch hash, and VerifyReplay requirements.

## VerifyReplay Checks

For every Linux child:

- call `VerifyReplay` with `base = ready_snapshot_ref`;
- use `Log::InputLogId(child.input_log_id.clone())`;
- set `bisect_on_divergence = Some(false)` for the acceptance path;
- count `EpochOk`;
- fail immediately on `Divergence`;
- fail on empty progress messages;
- require one `Done`;
- require `EpochOk` count equals parsed `EPOCH_HASH` count;
- require `Done.total_icount == parsed.header.end_icount`;
- require `Done.end_state_hash == child.state_hash`.

The acceptance summary printed to stderr should include:

```text
M7 Linux fork/verify progress: <verified>/<jobs>
```

At the end, print:

```text
M7 Linux fork/verify done: verified=<jobs> divergence=0 unique_hashes=<n> epoch_hashes=<n>
```

Do not make `unique_hashes == jobs` part of Linux correctness unless the implementation adds a Linux child workload that intentionally consumes fork entropy.
