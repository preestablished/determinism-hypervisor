# Suggestions

## `crates/dh-worker/tests/m5_record_replay.rs:637`

`expected_corpus_text` writes `records_applied` into `expected.txt`, and the checked-in manifest carries it at `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/expected.txt:16`, but the verifier never reads that expected value. `replay_once` still checks the replay outcome against `seconds - 1` at `crates/dh-worker/tests/m5_record_replay.rs:538`, so replay correctness is covered; the suggestion is to either compare the outcome to `expected_u64(expected, "records_applied")` as well or remove the unused manifest field so every declared expected value is load-bearing.

## `crates/dh-worker/tests/m5_record_replay.rs:788`

The ignored re-baseline test returns success when `DH_WORKER_REGEN_RR_CORPUS` is absent. That is safe for avoiding accidental fixture rewrites, but it also means an explicitly selected regeneration command missing the env var can report green while doing no re-baseline. Consider making the missing-env branch fail loudly for this specific ignored test, while keeping the current env guard before the four fixture writes at `crates/dh-worker/tests/m5_record_replay.rs:801`.

## `crates/dh-worker/tests/m5_record_replay.rs:263`

`expected_map` accepts extra keys, and `assert_log_matches_expected` only checks epoch keys that appear in the DHILOG at `crates/dh-worker/tests/m5_record_replay.rs:689`. The current checked-in `expected.txt` is coherent, but a future hand edit could leave stale extra fields without test failure. Consider asserting the expected key set for this single fixture format so `expected.txt` stays a strict manifest rather than a partially consumed note file.
