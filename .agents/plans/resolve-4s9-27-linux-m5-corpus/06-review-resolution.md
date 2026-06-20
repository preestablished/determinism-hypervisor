# Review Resolution

Two subagents reviewed the plan on 2026-06-20.

## Reviewers

- `019ee502-b242-7513-87ad-cb881901300e` (`Helmholtz`): acceptance correctness review.
- `019ee502-dd39-7d41-bb60-bde60d712f45` (`Hilbert`): implementation feasibility and repo-fit review.

Both reviewers returned `REQUEST_CHANGES`.

The full review summaries are in:

- `07-review-acceptance-correctness.md`
- `08-review-implementation-feasibility.md`

## Changes Applied

- Made stored DHILOG fetching and `LogReader` parsing mandatory. The plan now requires `snapstore_types::LogId::from_bytes` plus `snapstore_manifest::input_log::InputLogContainer::decode`, matching existing worker tests.
- Removed the fallback language that allowed `VerifyReplay`-only evidence. The plan now requires parsed epoch hashes and streamed `EpochOk` counts to match.
- Added `determinism_class_lock_blake3` to Linux corpus manifest requirements and Beads close evidence.
- Made the post-READY workload proof mandatory by requiring the existing `PVBLKIO1` meta checksum or an equivalent guest-visible proof.
- Renamed the optional regeneration test so its function name does not contain `linux`, preventing the required `linux` acceptance filter from selecting it.
- Changed the nanokernel ignored regression command to target `m5_accept_record_replay_60s_vns_pad_sequence_x100` by name, avoiding regeneration tests that intentionally panic without their regeneration env vars.

## Remaining Judgment

The plan still allows the implementation agent to choose lightweight manifest storage rather than full Linux snapshot/log fixture storage after measuring file size. That is intentional: the M9 Linux artifacts are staged external fixtures, and committing large Linux root snapshots may be worse than a host-pinned manifest gate.
