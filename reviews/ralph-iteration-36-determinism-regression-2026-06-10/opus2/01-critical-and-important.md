# Critical and Important Findings

**None.**

I actively tried to break this gate and could not. Documenting the negative results, because for a P0 determinism gate the *absence* of these failures is the load-bearing finding.

## Flake torture (the headline)

Ran `one_billion_instructions_twice_equal_final_hash` **5 times in a row**, each timed. All 5 passed; no divergence, no panic, no non-zero exit. Per the prompt, any flake here would have been Critical — there was none. Internal test times 4.38s–5.38s (each invocation does 2x 1e9 cold boots).

## Thread-routing / PMU-exhaustion torture

- `--test-threads=1`: both tests run sequentially on one worker thread. Each `cold_run` opens its own `InstRetired` counter, routes overflow to `gettid()` of the *current* worker thread, and the counter/slot are dropped at end of `cold_run` scope (fds closed) before the next run. Two `cold_run`s in one test function therefore never hold two counters simultaneously. PASS.
- Default threads (parallel): both test functions run concurrently → up to **2 pinned PMU counters live at once on a 2-core box** (4 counter open/close events total). No PMU exhaustion, no cross-thread routing leak, identical hashes. PASS. This is the strongest evidence that `route_overflow_to_thread(gettid(), ...)` correctly targets the owning worker thread rather than the process leader.

The `gettid()` rationale comment (regression.rs:36-45) is **correct and non-contradictory** after its partial rewrite: main-thread `tid == pid` is indeed not guaranteed for libtest worker threads, so routing must use the real per-thread tid via the syscall. No leftover/contradictory text.

## CI wiring is correct

- The arm lane correctly **excludes** `determinism-tests` (`ci.yaml:38`) alongside `dh-vmm`/`dh-worker`/`dh-cli` — consistent, the crate transitively needs x86_64-only dh-vmm. YAML parses clean.
- The `kvm-intel` lane runs `cargo test --workspace` with **no exclude** (`ci.yaml:78-`), so the 1e9 gate executes live there. The lane hard-fails if `/dev/kvm` is not rw (`ci.yaml:101`), so the gate **cannot silently self-skip and stay green** — exactly the failure mode you want closed for a required check.

## Full-workspace integration

`cargo test --workspace` ran green **twice** (170 tests each pass), proving the new crate does not break the build graph or contend destructively with the other live-KVM suites in the workspace.
