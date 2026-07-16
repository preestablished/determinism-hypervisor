# Positive Notes

## P-1. The `<= 1` threshold is matched to the *actual attr*, not a generic recipe

This is the standout part of the change. The naive "perf works if paranoid <= 2"
recipe would be **wrong here**, and the author got it right:

`perf_event_paranoid` semantics (kernel `Documentation/admin-guide/perf-security.rst`):

| level | unprivileged per-task (`pid=0, cpu=-1`) event with `exclude_kernel=0` |
|---|---|
| -1, 0, 1 | **allowed** (level 1 only forbids *CPU-wide* `cpu>=0` events) |
| 2 | **denied** — forces `exclude_kernel` for unprivileged users |
| 4 (Debian/Ubuntu downstream) | denied — all unprivileged `perf_event_open` |

The counter at `counter.rs:52-78` opens **per-task** (`pid=0, cpu=-1`, so the
level-1 CPU-wide restriction does not apply) and leaves **`exclude_kernel = 0`**
(comment line 63: "guest user+kernel count"). At `paranoid=2` an unprivileged
user opening this exact attr is rejected with EACCES *precisely because*
`exclude_kernel` is not set. Therefore `<= 1` — not `<= 2` — is the correct
cutoff. The `exclude_host`/`exclude_hv` filtering narrows *what* is counted but
does not relax the kernel-profiling gate. This is subtle and correct.

Empirically confirmed on this box: `paranoid=1`, euid=1000 (non-root), both PMU
tests pass.

## P-2. The `euid == 0` arm is a correct, minimal escape hatch

`CAP_PERFMON` / root bypasses `perf_event_paranoid` entirely. Gating on
`geteuid() == 0` lets a root-run CI lane (or a root lab box) keep asserting the
counter grant even under a stricter paranoid level, without weakening the
non-root path. Cheap, infallible, and the right capability proxy for a test
helper (a full `CAP_PERFMON` check would be overkill here).

## P-3. Comments explain the *why*, anchored to spec sections

Both edits carry rationale that survives the original author leaving: the
`counter.rs` comment names the §7.4 `paranoid=1` provisioning and the EACCES-at-4
failure it prevents; the `ci.yaml` comment explains exactly why `--all` is wrong
(sibling path deps) rather than just what the replacement does. This is the kind
of comment that stops a future "cleanup" from reverting the fix.

## P-4. `#[allow(unsafe_code)]` + `// SAFETY:` matches established crate convention

`lib.rs` declares `#![deny(unsafe_code)]` crate-wide; every `unsafe` block in
`counter.rs` (8 pre-existing: `perf_event_open`, `from_raw_fd`, the ioctls,
`fcntl`, `read`) uses the targeted `#[allow(unsafe_code)]` + `// SAFETY:` pattern.
The new `geteuid` block follows it exactly. The SAFETY note ("geteuid has no
preconditions and cannot fail") is accurate — `geteuid(2)` is documented as always
successful.

## P-5. Skipping on hosted runners does not weaken the suite

The two PMU tests still run for real on the `kvm-intel` self-hosted lane
(`paranoid=1` per §7.4) and on the lab box — exactly the environments where a PMU
exists and the guest-only counter contract matters. Hosted runners (`paranoid=4`,
no `/dev/kvm`) could never meaningfully exercise a *guest-mode* counter anyway, so
self-skipping there loses no coverage. The KVM-gated lane remains the source of
truth, matching the file's own header comment (lines 67-69) and the iteration-20
split's intent. No reduction in real assurance.

## P-6. No manifest churn

`libc` is already a regular dependency of `dh-detclock`
(`crates/dh-detclock/Cargo.toml`), so `libc::geteuid()` adds nothing to the
dependency graph and needs no `dev-dependencies` entry — the helper lives in
`#[cfg(test)]` but the crate is depended on regardless. Clean.
