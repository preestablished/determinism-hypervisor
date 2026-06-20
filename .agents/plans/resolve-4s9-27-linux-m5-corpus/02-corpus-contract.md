# Linux Corpus Contract

## Required Proof

The Linux M5 corpus must prove the same core properties as the nanokernel M5 corpus, through the worker API:

- The base snapshot is a real M9 Linux READY snapshot, not a boot-to-READY recording used as the replay segment.
- The recorded segment begins after READY and runs deterministic guest work.
- The sealed DHILOG contains at least one `EPOCH_HASH`.
- `VerifyReplay` emits one or more `EpochOk` progress messages and then `Done`.
- `VerifyReplay Done.end_state_hash` equals the live post-segment snapshot state hash.
- The DHILOG END state hash equals that same value.
- The recorded log can be parsed and its metadata matches a pinned expected manifest.
- The segment proves real post-READY guest work through a mandatory guest-visible proof, either the existing meta pv-blk checksum or an equally strong frame/BLKO proof recorded in the manifest.

## Segment Shape

Use a short, deterministic post-READY segment. Recommended initial shape:

- Start from `common::m9_linux_ready_snapshot("m5_record_replay::linux_m5_record_replay_corpus", 2)`.
- Run one or more post-READY frame budgets, not raw icount, so the segment naturally includes the M9 workload's pv-pad frame marks.
- Start with `FrameBudget(2)` and a hard cap comfortably above observed frame timing, for example `100_000_000`.
- If the resulting DHILOG does not contain at least one `EPOCH_HASH`, increase the budget to `FrameBudget(4)` or adjust the test machine config epoch behavior through existing config seams only if that is already supported locally.

Rationale: The current M9 workload frame 0 performs pv-blk IO, and every frame emits `FRAME_MARK`. A frame-budget segment is a deterministic input script because the stop condition is guest-emitted frame marks, not wall-clock time.

## Corpus Storage Policy

There are two acceptable storage modes. The implementation agent should measure sizes before choosing.

### Preferred: Lightweight Checked-In Manifest

Use this mode if full Linux root snapshot/log fixtures are too large for the repository.

Checked-in files under `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/`:

- `README.md`
- `expected.txt`

`expected.txt` pins:

- `name=m9_linux_post_ready`
- `determinism_class_lock_blake3`
- artifact hashes for bzImage, initramfs, base image, game image
- `mem_bytes`
- `machine_config_hash`
- `ready_snapshot_ref` if stable within the recorded fixture mode, otherwise omit and explain why in README
- `run_until=frame_budget:<n>`
- `hard_icount_cap=<n>`
- `dhilog_blake3`
- `dhilog_records`
- `records_applied`
- `epoch_hashes_verified`
- `end_icount`
- `end_vns`
- `end_state_hash`
- `frame_counter`
- each `epoch_<index>=<icount>:<hash>`
- mandatory post-READY proof fields from the workload meta region, such as pv-blk checksum, or an equivalent frame/BLKO proof

The test records the segment live on this KVM host and asserts the live metadata matches `expected.txt`. This is acceptable because the full M9 artifacts are external staged inputs and already too large to be normal source fixtures.

### Full Checked-In Corpus

Use this mode only if size is reasonable and repo policy allows it.

Checked-in files:

- `root-sparse.bin`
- `root.dhsnap`
- `recording.dhilog`
- `expected.txt`
- `README.md`

Mirror the nanokernel `pad_echo_6s` load/reverify pattern. This mode gives stronger offline reverify but may be impractical for a 128 MiB Linux guest snapshot and BzImage-backed fixture.

## Regeneration Guard

If any corpus file is generated or updated, gate regeneration with an explicit env var:

```bash
DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1
```

The regeneration test must panic when that variable is absent. A normal test run should reverify or live-record-and-compare only; it should not rewrite fixtures.

## DHILOG Assertions

After the segment snapshot, fetch the stored DHILOG payload from snapstore and parse it with `LogReader`.

Required assertions:

- `reader.header().has_epoch_hashes()`.
- epoch count from `RecordBody::EpochHash` is greater than zero.
- `reader.header().end_state_hash` equals the snapshot response state hash.
- `reader.end()` state hash equals the snapshot response state hash.
- `reader.header().base_snapshot_id` equals the READY snapshot ref.
- `reader.header().machine_config_hash` equals `ready.config_hash`.
- `reader.header().record_count` equals the expected manifest value.
- `reader.header().end_icount` and `end_vns` equal expected manifest values.
- every epoch hash line in `expected.txt` matches the parsed log.

Fetching and parsing the stored DHILOG is mandatory. Do not replace this with a `VerifyReplay`-only check; `4s9.27` requires evidence for every recorded `EPOCH_HASH`, and that evidence comes from parsing the log plus matching the streamed `EpochOk` count.

## VerifyReplay Assertions

Do not use only `common::verify_replay_done`; add or localize a helper that counts progress:

```rust
struct VerifyReplayEvidence {
    done: proto::VerifyDone,
    epoch_ok_count: usize,
}
```

The helper should:

- stream `VerifyReplay`;
- increment for `EpochOk`;
- fail on `Divergence`;
- fail on empty messages;
- return `Done` and the count;
- assert no `EpochOk` appears after `Done`.

Required test assertions:

- `epoch_ok_count > 0`.
- `epoch_ok_count == parsed_epoch_hash_count`.
- `done.end_state_hash == post_segment_snapshot.state_hash`.
- `done.total_icount == parsed end icount` if the proto field is available and named as expected.

## Linux Selection Guard

The Linux test must stay `#[ignore]` and include `linux` in its name. The command:

```bash
cargo test -p dh-worker --test m5_record_replay linux -- --ignored --list
```

must list the real Linux test, not the old guard.

Remove `linux_m5_record_replay_requires_real_linux_corpus` when the real test lands.
