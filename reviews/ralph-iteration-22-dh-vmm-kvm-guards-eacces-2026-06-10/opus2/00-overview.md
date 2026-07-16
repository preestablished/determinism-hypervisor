# Review Overview — iteration 22: dh-vmm KVM test guards (EACCES)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-22-dh-vmm-kvm-guards-eacces` vs `main`
- **Bead:** determinism-hypervisor-vwr
- **Scope:** 4 files, +25/-5 lines, test-guard-only change

## What the change does

Live-KVM tests previously gated themselves on `/dev/kvm` *existence*
(`Path::new("/dev/kvm").exists()`). GitHub `ubuntu-latest` exposes the node
(nested virt) but denies the runner user access, so `Kvm::new()` fails with
`EACCES` *after* the existence check passes — the live tests ran and panicked
in CI (run 27251695143). This change switches the guard to an **rw-open probe**:
a new `#[cfg(test)] pub(crate) fn kvm_usable()` in `dh-vmm/src/kvm.rs` that
`OpenOptions::new().read(true).write(true).open("/dev/kvm").is_ok()`. The three
`dh-vmm` test modules (`kvm.rs`, `msr.rs`, `run.rs`) re-point their local
`kvm_available()` helper at it; `dh-worker/src/preflight.rs` gets an inline copy
of the same probe in `full_preflight_passes_on_configured_host`.

## Verdict

**Approve with minor follow-ups.** The core mechanism is correct and the change
achieves its goal: hosted CI will now skip cleanly while the kvm-intel lane and
the lab box keep running the live legs. I verified the central
semantic-equivalence claim (the probe's `O_RDWR` matches the flags
`kvm-ioctls 0.24` uses inside `Kvm::new()`), and I confirmed on this rw lab host
that all 13 live tests + `full_preflight` **run** (do not skip) and pass.

No coverage is lost on hosted CI: every pure-logic test
(`pio_classification_table`, `mmio_hole_covers_device_windows`,
`allow_ranges_cover_exactly_the_chosen_set`, `policy_is_fixed_and_total`, all
five preflight parser tests) is **ungated** and unchanged — they were never
behind a KVM guard and still are not.

The remaining concerns are not blockers but should be tracked: a **skip-vs-fail
asymmetry** (a transient open failure such as `EMFILE`/`ENFILE`/`EINTR` on the
lab box or the kvm-intel lane silently shrinks live coverage instead of failing
loud), a naming **double-indirection** (`kvm_available` → `kvm_usable`), and
**cross-crate duplication** of the probe into `dh-worker`.

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 2 |
| Suggestions | 4 |
| Positive notes | 5 |

## Verification performed

- Read `/tmp/ralph-iter22-diff.txt` and all four full files at their guard sites.
- Confirmed `kvm-ioctls 0.24.0` `Kvm::new()` opens `/dev/kvm` with `O_RDWR`
  (`~/.cargo/registry/.../kvm-ioctls-0.24.0/src/kvm_ioctls.rs:296`) — the probe's
  read+write maps to the same flags, so an open that succeeds for the probe also
  succeeds for the real path (modulo `O_CLOEXEC`/fd-exhaustion races, see 01).
- `cargo test -p dh-vmm -p dh-worker`: 40 + 7 + 1 tests, **0 failed, 0 ignored**;
  the 7 live `dh-vmm` legs (`caps_gate`, `forbidden_list`, `slot_vm`,
  `mem_above_hole`, both `denied_*`, `allowed_msr`, both `run.rs` kick tests) and
  `full_preflight` all ran (no "skipping" lines) on this rw host.
- Confirmed branch diff scope is exactly the 4 described files, guard-bodies only —
  no production code touched.
