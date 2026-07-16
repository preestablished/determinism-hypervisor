# Action Items

### Critical

- [ ] None.

### Important

- [ ] **I-1: Tighten the `kvm-intel` CI lane gate to rw semantics.** In
  `.github/workflows/ci.yaml:94`, the live-KVM lane currently gates with read-only
  `test -r /dev/kvm`. The new test guards require an rw open, so a read-but-not-write
  runner would pass the lane gate while every live-KVM test silently self-skips,
  turning the lane green without exercising the hypervisor. Add a write check so the
  job fails loudly when `/dev/kvm` is not rw-usable:
  ```yaml
  - name: Assert /dev/kvm is rw-usable (matches the test guards' rw-open probe)
    run: |
      test -c /dev/kvm || { echo "::error::/dev/kvm missing on runner"; exit 1; }
      test -r /dev/kvm && test -w /dev/kvm \
        || { echo "::error::/dev/kvm not rw-usable; live-KVM tests would silently skip"; exit 1; }
  ```
  Out of this diff's scope (CI config, not the reviewed crates); safe as a fast-follow
  since the current runner has rw access. Recommend a bd issue.

### Suggestions

- [ ] **S-1:** If a third crate ever needs the rw-open probe, promote a shared
  `kvm_rw_openable()` helper into a low-level or test-support crate instead of copying
  the body again. Two copies (`dh-vmm::kvm_usable` + `dh-worker` inline) is acceptable
  given the cross-crate `cfg(test)` limitation; three would not be.
- [ ] **S-2:** Optionally delete the thin `kvm_available()` forwarders in
  `dh-vmm/src/{kvm,msr,run}.rs` in favor of direct `crate::kvm::kvm_usable()` calls.
  Reviewer recommends keeping them (better local readability, smaller future churn) —
  noted only for completeness.
- [ ] **S-4 (cosmetic):** Align self-skip log wording. `dh-vmm` sites still print
  `"skipping: no /dev/kvm"` while the predicate is now "not rw-openable"; `dh-worker`
  prints `"skipping: /dev/kvm not usable"`. Consider standardizing on
  `"skipping: /dev/kvm not rw-usable"` across all guard sites.
