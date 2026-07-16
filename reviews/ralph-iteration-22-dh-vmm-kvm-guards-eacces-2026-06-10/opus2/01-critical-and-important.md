# Critical & Important Findings

## Critical

None. The change is test-infrastructure only, semantically sound, and verified
to behave correctly on a usable host.

---

## Important

### I-1 — Skip-vs-fail asymmetry: a transient open failure silently shrinks live coverage on the lab box and the kvm-intel lane

**Files:** `dh-vmm/src/kvm.rs:406-412` (`kvm_usable`),
`dh-worker/src/preflight.rs:320-328` (inline probe).

`kvm_usable()` collapses **every** open error into "skip the test":

```rust
pub(crate) fn kvm_usable() -> bool {
    std::fs::OpenOptions::new()
        .read(true).write(true).open("/dev/kvm")
        .is_ok()   // <- ANY Err => skip
}
```

The intended skip case is `EACCES` (hosted CI denies the runner user) and
`ENOENT` (no node). But `is_ok()` also swallows **transient / environmental**
failures that have nothing to do with KVM being unavailable:

- `EMFILE` — the process hit its per-process fd limit (`RLIMIT_NOFILE`).
- `ENFILE` — system-wide open-file table exhausted.
- `EINTR` — `open()` interrupted by a signal. Note `run.rs` deliberately raises
  signals at the current thread (`kick_signal()` machinery); a probe call racing
  a pending kick is not purely hypothetical in this codebase.
- `ENOMEM`, `EBUSY`, and similar.

On the lab box and the kvm-intel lane — the exact hosts whose live coverage this
change is meant to preserve — any of these makes the test **silently return
green without exercising KVM at all**. The whole point of iteration 22 is that
"missing access" and "ran the live path" must be distinguishable; folding
transient failures into the skip bucket reintroduces a quieter version of the
original problem (a live test that no longer asserts anything, but now passes
instead of failing).

**The asymmetry is the core risk:** the *previous* `exists()` check failed loud
(panic in `Kvm::new()`) on the configured hosts, which is noisy but safe — a
broken live host shows up red. The new probe fails *quiet* on those same hosts.
The fix traded a false-red on CI for a potential false-green on the lab box.

**Recommendation:** distinguish errnos. Skip only on the access-class errors;
let everything else fall through so the test still attempts `Kvm::new()` and
fails loudly if the host is genuinely broken:

```rust
pub(crate) fn kvm_usable() -> bool {
    use std::io::ErrorKind;
    match std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm") {
        Ok(_) => true,
        Err(e) => match e.kind() {
            // Hosted CI / no node: legitimately skip.
            ErrorKind::PermissionDenied | ErrorKind::NotFound => false,
            // EMFILE/ENFILE/EINTR/etc. on a host that is *supposed* to have
            // KVM: do NOT mask it as "unavailable" — let the live test run
            // and surface the real failure.
            _ => true,
        },
    }
}
```

(`raw_os_error()` against `libc::EACCES`/`ENOENT` is equally fine and matches the
errno vocabulary used elsewhere in the crate.) At minimum, if the maintainers
prefer to keep `is_ok()`, the rationale for swallowing transient errnos should be
written into the doc comment so the trade-off is a documented decision rather
than an accident.

---

### I-2 — The probe is duplicated into `dh-worker` by copy-paste, with a divergent skip message and no shared source of truth

**Files:** `dh-vmm/src/kvm.rs:406-412` vs
`dh-worker/src/preflight.rs:320-328`.

The same rw-open logic now exists in two crates as two independent literal
copies. They have already drifted in one observable way — the skip message:

- `dh-vmm` tests (unchanged): `eprintln!("skipping: no /dev/kvm")`
- `dh-worker` (this change): `eprintln!("skipping: /dev/kvm not usable")`

So within a single CI log the same condition prints two different strings, and
the `dh-vmm` message ("no /dev/kvm") is now actively *misleading*: the node may
well exist — the probe skipped because access was denied, not because the node is
absent. The old message was accurate under `exists()`; under the new
access-probe it is wrong.

This is a maintainability trap: I-1's errno fix (or any future tweak to the
probe semantics) now has to be applied in two places, and a fix to one without
the other silently diverges the two lanes' skip behavior. `dh-worker` already
depends on `dh-vmm` (it imports `dh_detclock`/KVM types transitively), so the
duplication is avoidable.

**Recommendation (pick one):**
- Promote a single probe to a shared spot both crates can call. `kvm_usable()`
  is currently `#[cfg(test)] pub(crate)` inside `dh-vmm`, so `dh-worker` cannot
  see it across the crate boundary in a test build. Either expose a tiny
  `#[doc(hidden)] pub fn kvm_usable_for_tests()` from `dh-vmm`, or put the probe
  in a shared test-support module/crate.
- If the duplication is intentional (avoid widening `dh-vmm`'s public surface),
  then at least **unify the skip message** to one accurate string —
  `"skipping: /dev/kvm not usable (rw open failed)"` — across all five call
  sites, and add a one-line comment in each crate pointing at the other as the
  canonical twin so future edits stay in sync.
