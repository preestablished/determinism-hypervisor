# 02-suggestions.md

Suggestion: `tests/determinism/tests/linux_ready.rs:208` only validates Ready payload shape/equality. The broader M9 docs define Ready as including Hello/autostart/control and expected regions before Ready, but this test would accept a well-formed Ready with `region_count = 0`. If this direct test is intended to prove the stronger READY semantic, add event ordering checks similar to `crates/dh-worker/tests/linux_worker_api.rs::assert_ready_ordering`; otherwise note this as intentionally narrower than the worker API gate.

Suggestion: `tests/determinism/tests/linux_ready.rs:159` stores the first Ready event but does not reject duplicate Ready events drained at the same boundary. Consider asserting exactly one Ready event before the stop boundary to avoid masking a guest/sdk regression.
