# Suggestions (Non-blocking)

### S-1 — Probe duplication between `dh-vmm::kvm_usable` and the `dh-worker` inline copy

- **Files:** `crates/dh-vmm/src/kvm.rs:406-412`, `crates/dh-worker/src/preflight.rs:320-328`
- The rw-open probe now exists in two places. The duplication is *justified* —
  `#[cfg(test)]` items aren't visible across crate boundaries, and the comment in
  `kvm.rs` documents this — so I am not requesting a change. If a third crate ever
  needs the same probe, consider promoting a tiny non-`cfg(test)` helper (e.g.
  `pub fn kvm_rw_openable() -> bool`) into a shared low-level crate, or a
  `dev-dependencies` test-support crate, rather than copying the body a third time.
  Two copies is fine; three is a smell.

### S-2 — `kvm_available()` wrappers are now thin pass-throughs

- **Files:** `crates/dh-vmm/src/kvm.rs:418-420`, `msr.rs:126`, `run.rs` (same shape)
- Each module's `fn kvm_available() -> bool { crate::kvm::kvm_usable() }` is now a
  one-line forwarder. The indirection is harmless and keeps the call sites
  (`if !kvm_available()`) untouched, which minimizes the diff. Optionally, the three
  wrappers could be deleted and call sites changed to `if !crate::kvm::kvm_usable()`
  directly, removing a layer of naming. I lean toward **keeping** the wrappers: the
  local name reads better at the guard sites and shrinks future churn. Listed only
  for completeness — either choice is defensible.

### S-3 — Naming consistency between the two probes

- The `dh-vmm` helper is named `kvm_usable()`; the `dh-worker` copy is anonymous
  (inline `if`). If S-1's shared helper is ever adopted, give both the same name
  (`kvm_usable` / `kvm_rw_openable`) so a grep finds every gate. Minor.

### S-4 — Skip messages differ in wording

- `dh-vmm` sites print `"skipping: no /dev/kvm"` (unchanged) while the now-rw-gated
  `dh-worker` test prints `"skipping: /dev/kvm not usable"`. The `dh-vmm` wording is
  slightly stale now that the predicate is "not rw-openable" rather than "absent".
  Cosmetic — consider aligning to `"skipping: /dev/kvm not rw-usable"` everywhere so
  CI logs read consistently when tests self-skip on hosted runners.
