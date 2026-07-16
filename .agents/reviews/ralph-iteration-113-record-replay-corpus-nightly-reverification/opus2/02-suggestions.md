# Suggestions

## Suggestion: bound sparse-root count before allocation

- File: `crates/dh-worker/tests/m5_record_replay.rs:207`
- File: `crates/dh-worker/tests/m5_record_replay.rs:213`
- File: `crates/dh-worker/tests/m5_record_replay.rs:217`

`decode_sparse_root` reads `count` and immediately uses it for `Vec::with_capacity(count as usize)`, then checks each page index inside the loop. The committed fixture is valid and hash-pinned, but future corpus updates are still committed binary input that the self-hosted nightly will parse. Reject `count > MEM / PAGE_SIZE` and, ideally, validate the exact encoded length before allocating.

## Suggestion: add a tight timeout to the corpus nightly job

- File: `.github/workflows/nightly-drift.yaml:78`
- File: `.github/workflows/nightly-drift.yaml:105`

The new corpus leg should normally finish quickly; the local exact command completed in about one second after compilation. A `timeout-minutes` on `record-replay-corpus` would keep a bad KVM/kernel state, a stuck store subprocess, or a future replay hang from occupying the single `kvm-intel` runner until GitHub's much larger default timeout.

## Suggestion: make the corpus fixture shape testable without KVM

- File: `crates/dh-worker/tests/m5_record_replay.rs:738`
- File: `crates/dh-worker/tests/m5_record_replay.rs:761`

The current verifier returns before reading any fixture files when `/dev/kvm` is unavailable. That is right for replay, but it means ordinary hosted/non-KVM CI cannot catch missing fixture files, malformed `expected.txt`, bad sparse-root framing, or DHILOG parse failures. Consider splitting the fixture/manifest/hash validation into a non-KVM test and keeping only the actual replay in the KVM-gated test.

## Suggestion: install the kick handler explicitly in the corpus replay test

- File: `crates/dh-worker/tests/m5_record_replay.rs:772`
- File: `crates/dh-worker/src/replay_engine.rs:84`
- File: `crates/dh-vmm/src/boundary.rs:91`

The exact corpus test passed locally without this, likely because pad-echo exits frequently enough that the replay does not rely on a perf overflow kick in this path. Still, `replay_segment`'s lower-level boundary contract says the kick handler is a precondition. Calling `dh_vmm::run::install_kick_handler().unwrap()` in `record_replay_corpus_pad_echo_6s_reverifies` would keep the corpus-only path explicit instead of relying on guest exit density or process-global side effects from other tests.

