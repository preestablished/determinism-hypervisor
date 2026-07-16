# Suggestions (non-blocking)

### S-1. Prefer a manifest-derived member list that does not pass through a JSON pipeline

**File:** `.github/workflows/ci.yaml:58`

The `cargo metadata | jq | sed | tr` chain is the fragile part of this fix (see
I-1). The set it is trying to reproduce is literally the `members = [...]` list
already spelled out in the root `Cargo.toml`. Two lower-risk alternatives:

- Single jq template, no `sed`/`tr`, with pipefail:
  `jq -r '.packages[].name | "--package " + .'` then `xargs cargo fmt --check`.
- Or skip metadata entirely: `cargo fmt --check -p dh-detclock -p dh-devices …`
  pinned to the member list, or a `for d in crates/* tools/dh-cli; do (cd "$d" && cargo fmt --check); done`.
  An explicit list drifts only when crates are added — and a missing crate then
  fails *loudly* in a different lane (clippy/build over `--workspace`), unlike
  the silent no-op.

Trade-off noted: an explicit list needs maintenance when crates are added.
That is a deliberate, reviewable cost vs. a silent gate-removal. Your call.

### S-2. `pmu_available()` ignores CAP_PERFMON — a non-root user with the cap is wrongly skipped

**File:** `crates/dh-detclock/src/counter.rs:241-245`

The gate is `level <= 1 || euid == 0`. A user who is **not root** but holds
**`CAP_PERFMON`** (or legacy `CAP_SYS_ADMIN`) can open this event even at
paranoid 2–4, yet the gate would return `false` and *skip* the test for them.

For the **lab box this is moot** — §7.4 provisions paranoid=1, so `level <= 1`
already passes regardless of caps, and the box is the only place the test is
meant to actually run. So this is a fidelity gap, not a bug that bites today.
If you ever run CI as a capability-scoped (non-root) user on a hardened host, the
gate would under-report. If you want to close it, the honest signal is not to
*predict* the policy at all but to **attempt the open** and treat `EACCES`
specifically as skip (see S-3), which captures CAP_PERFMON, paranoid level, and
seccomp/LSM denials in one shot.

### S-3. Consider "try the open, skip only on EACCES" instead of modelling the policy

**File:** `crates/dh-detclock/src/counter.rs:231-246`

`pmu_available()` re-derives, in user space, the kernel's own accept/deny
decision for this attr (paranoid level + euid, but not caps, not seccomp, not
container LSM). That duplication is why S-2 exists and why the `<=1` threshold
had to be reasoned about so carefully. A more robust shape:

```rust
match InstRetired::open_for_current_thread() {
    Ok(_) => { /* run the real assertions */ }
    Err(CounterError::Open(e)) if e.contains("EACCES") || e.contains("Permission denied")
        => { eprintln!("skipping: perf policy denies counter ({e})"); }
    Err(e) => panic!("§7.4 host must grant a counter: {e:?}"),
}
```

This makes the skip condition *exactly* the kernel's denial, eliminates the
paranoid-level parsing and the euid/CAP_PERFMON modelling, and keeps a non-EACCES
failure (ENODEV, EMFILE, NotPinned) a hard failure. Caveat: distinguish EACCES by
errno rather than substring if you adopt this — `CounterError::Open` currently
carries a formatted string; matching `.raw_os_error() == Some(libc::EACCES)`
would require threading the errno through, a small refactor. Optional.

### S-4. `unwrap_or(i32::MAX)` on parse failure is a sound fail-safe — document the intent inline

**File:** `crates/dh-detclock/src/counter.rs:241`

```rust
let level: i32 = s.trim().parse().unwrap_or(i32::MAX);
```

This is the *correct* default: an unparseable paranoid file → `i32::MAX` →
`level <= 1` is false → skip (conservative). Good. The only realistic way to hit
it is a kernel that one day writes something non-integer there (none does today;
the value is `-1..=4`). Worth a half-line comment — `// unparseable => MAX =>
treat as strictest => skip` — so a future reader doesn't "simplify" it to
`unwrap_or(0)` (which would flip it to fail-open and run the test on a box where
perf is actually locked down). Pure maintainability guard.
