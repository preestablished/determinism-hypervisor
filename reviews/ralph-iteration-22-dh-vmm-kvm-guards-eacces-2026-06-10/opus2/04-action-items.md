# Action Items

### Critical

_None._

### Important

- [ ] **(I-1) Distinguish skip-class errnos from transient failures in the probe.**
  In `dh-vmm/src/kvm.rs:406-412` (`kvm_usable`) and the inline copy at
  `dh-worker/src/preflight.rs:320-328`, stop folding every open error into "skip".
  Skip only on `EACCES`/`PermissionDenied` (hosted CI) and `ENOENT`/`NotFound`
  (no node); for any other errno (`EMFILE`, `ENFILE`, `EINTR`, `ENOMEM`, …) return
  `true` so the live test still runs and fails loudly on a host that is supposed
  to have KVM. Without this, a transient open failure on the lab box or the
  kvm-intel lane turns a live test into a silent green no-op — reintroducing a
  quiet version of the bug this iteration set out to fix. If `is_ok()` is kept
  deliberately, document the swallowed-errno trade-off in the doc comment.

- [ ] **(I-2) Remove the cross-crate probe duplication or unify it.**
  The rw-open probe is copy-pasted into `dh-worker/src/preflight.rs` and has
  already drifted from the `dh-vmm` original in its skip message. Either expose a
  single shared probe (`#[doc(hidden)] pub fn kvm_usable_for_tests()` from
  `dh-vmm`, or a shared test-support module) that both crates call, or — if the
  duplication is intentional — unify the skip message to one accurate string
  across all five call sites and add a comment in each crate pointing at its twin
  so future edits stay in sync. As-is, any I-1 fix must be applied twice or the
  two lanes silently diverge.

### Suggestions

- [ ] **(S-1) Collapse the `kvm_available()` → `kvm_usable()` indirection.**
  Delete the three forwarding helpers in `kvm.rs:418`, `msr.rs:126`, `run.rs:138`
  and call `crate::kvm::kvm_usable()` directly at the guards (or `use … as …`).
  Settle on one name — prefer `kvm_usable`, since "existence ≠ usability" is the
  lesson of this iteration.

- [ ] **(S-2) Fix the stale `dh-vmm` skip messages.**
  Update the eight `eprintln!("skipping: no /dev/kvm")` sites
  (`kvm.rs:425,439,460,476`; `msr.rs:169,214,263`; `run.rs:157,215`) to
  `"skipping: /dev/kvm not usable"`. Under the access-probe, "no /dev/kvm" is
  wrong — the node exists, access was denied.

- [ ] **(S-3) Note "usable ≠ compliant" on `kvm_usable`.**
  Add one line to the doc comment clarifying that the probe only checks
  openability; the live asserts below (`KvmSystem::open`, `create_slot_vm`,
  preflight) carry the §2.1/§7.4 compliance checks.

- [ ] **(S-4) Consider observable skip vs. ran for live legs (follow-up).**
  A skipped live test reports identical `test result: ok` to a real pass. If
  coverage-visibility on the kvm-intel lane matters, file a follow-up to gate the
  live legs behind `DH_REQUIRE_KVM=1` (panic instead of `return` when the probe
  fails) or `#[ignore]` + an explicit `--ignored` CI lane. Not required for this
  change.
