# Overview

Re-review target: branch `ralph/iteration-122-dh-cli-operator-verbs`

Scope reviewed:
- `tools/dh-cli/src/ops.rs`
- `tools/dh-cli/src/cli.rs`
- `tools/dh-cli/src/lib.rs`
- `tools/dh-cli/Cargo.toml`
- `Cargo.lock` for dependency edges only

Result: no Required findings remain.

The follow-up resolves the main concerns from the first pass:
- Replay/verify now stream progress records as they arrive and flush per message.
- JSON replay/verify output is JSON Lines: progress objects plus a final `ok` or `error` object from the dispatcher.
- Parse errors honor `--json` with a structured usage error object.
- Flag-like missing values and conflicting `--bisect`/`--no-bisect` are rejected.
- In-process fake `HypervisorWorker` tests now pin request fields for snapshot, restore, fork, replay, verify, and progress-before-late-error behavior.

Dependency boundary remains clean. `dh-cli` depends on `dh-proto`, `tonic`, `tokio`, and test-only `tokio-stream`; it still does not import `dh-worker` or fake local engine success.

Verification run:

```bash
cargo fmt --check --package dh-cli
cargo test -p dh-cli
git diff --check
```

Result: all passed.
