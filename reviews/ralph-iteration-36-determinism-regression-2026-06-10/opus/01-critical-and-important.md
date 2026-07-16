# Critical and Important Findings

## Critical

**None.**

## Important

**None.**

---

## Why nothing rises to Important — scrutiny notes

Each high-risk area called out in the review brief was checked and cleared:

### 1. Cold-boot semantics & cross-run state leakage — CLEAR
`cold_run` constructs a fresh `KvmSystem`, slot/VM, ELF load, `InstRetired` counter, and
`StateHashChain` on every call. There is no shared mutable state carried between the two
invocations in a single process:
- The kick handler (`dh_vmm::run::install_kick_handler`) is process-wide and idempotent —
  it installs the same `sigaction` for `SIGRTMIN+4` each call; re-installing the identical
  handler is a no-op in effect. Routing is per-thread via a `thread_local` `KICK_TARGET`
  cell, so a counter armed on thread T only ever kicks T's vCPU.
- The counter is opened `for_current_thread` and routed to `gettid()` of the running
  thread, so two concurrently-scheduled tests on different threads do not cross-route.
- The hash chain is fresh per run with identical fixed seeds `[7;32]`.

Conclusion: the two cold runs are independent; equality of the full 5-tuple (icount, rip,
rcx, vns, state_hash) is a genuine determinism assertion, not an artifact of shared state.

### 2. Budget vs guest-HLT margin — CLEAR (verified against the asm)
`landing_loop.asm` confirms the loop body is exactly 8 instructions/iter and the cmdline
`125000000` sets rcx = 125M → 1e9 loop instructions. Ahead of the loop sits a deterministic
prologue (cmdline digit-parse loop + LCG setup + `align 16` NOP pad); after it, a serial
`out` then crt0 HLT. Total retired instructions available = prologue + 1e9 + epilogue,
which is strictly greater than 1e9. Budget 1e9 therefore lands mid-loop, before the guest
can reach the `out`/HLT park. The test's `assert_eq!(out.reason, StopReason::BudgetReached)`
empirically confirms `GuestHalted` is never reached. The failure mode is loud by design: if
the iteration count were ever set below the budget the guest would HLT and the assert would
fail with `GuestHalted` instead of `BudgetReached`.

### 3. cargo test parallelism / PMU counter contention — CLEAR
The two tests may run concurrently on separate threads (two VMs, two per-thread-routed
counters). Pinned guest-only counters on different threads are schedulable on different
cores; same-core contention would surface as `NotPinned`/open errors, which did not occur
across multiple full runs. Suite completed in 3.5–4.0s with no counter errors.

### 4. CI timing — CLEAR
Debug run measured ~4.12s wall on this box; the kvm-intel runner is comparable hardware.
Adds ~4s to the kvm-intel lane on every push to main + PR. Acceptable.

### 5. Cargo.toml / libc placement — CLEAR
The final `tests/determinism/Cargo.toml` has `libc = "0.2.186"` as the last line under the
`[dev-dependencies]` table (the `[dependencies]` section above it is intentionally empty).
`libc` is correctly a dev-dependency (only the test uses it, for `SYS_gettid`). No stray
root key; `cargo build`/`clippy`/`test` all parse and succeed.

### 6. Determinism of the gate itself — CLEAR
All seed material is fixed (`[7;32]`), cmdline is constant, and thread scheduling is
irrelevant to the asserted outcome. Re-running the 1e9 gate 3x produced identical pass.

### 7. ci.yaml exclude correctness — CLEAR
`determinism-tests` dev-depends on `dh-vmm`, which the arm lane already excludes (x86-only).
Excluding `determinism-tests` on arm is both correct and necessary; the kvm-intel lane (via
`--workspace`) still runs the 1e9 gate.

### 9. Branch-protection wiring — OUT OF SCOPE (correct)
The bead notes the job "becomes required-for-merge from M3 onward (wiring bead below)";
branch protection is bead 8n7 and correctly not touched here.
