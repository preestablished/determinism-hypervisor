# Positive Notes

### P-1 — Correct root-cause diagnosis and the right fix shape

The change correctly identifies that `Path::exists()` is the wrong predicate:
the device node exists on hosted CI but `Kvm::new()` fails with `EACCES`
*after* the existence check passes, so the existence guard let the live test run
straight into a panic. Probing actual openability is the right gate. This is a
precise, minimal fix that addresses the real failure mode rather than papering
over the symptom.

### P-2 — Probe flags match the production open path exactly

The probe uses `read(true).write(true)` (→ `O_RDWR`), which is exactly what
`kvm-ioctls 0.24.0` `Kvm::new()` uses to open `/dev/kvm`
(`open(KVM_PATH, O_RDWR)`). An open that succeeds for the probe therefore
succeeds for the real path with respect to permissions — the semantic
equivalence the change relies on actually holds. A read-only probe would have
been subtly wrong (could pass where `O_RDWR` fails); the author got this right.

### P-3 — No pure-logic coverage lost on hosted CI

The concern that pure-logic tests might get newly hidden behind the KVM guard
does not materialize. `pio_classification_table`, `mmio_hole_covers_device_windows`,
`allow_ranges_cover_exactly_the_chosen_set`, `policy_is_fixed_and_total`, and all
five `dh-worker` preflight parser tests are **ungated** and untouched — they ran
everywhere before and still do. The change only edited the bodies of tests that
were already KVM-gated, plus the one inline preflight guard. Hosted CI keeps full
pure-logic coverage.

### P-4 — Production code untouched; blast radius is test-only

The diff is strictly `#[cfg(test)]` helpers and guard bodies. The new `kvm_usable`
is `#[cfg(test)] pub(crate)`, so it compiles out of release builds entirely and
cannot affect the hypervisor's runtime behavior. Low-risk by construction.

### P-5 — Documented rationale and verified locally

The doc comment on `kvm_usable` and the inline preflight comment both explain
*why* existence is insufficient (nested-virt node + denied runner user), which is
exactly the context a future maintainer needs. I independently confirmed on this
rw lab host that all 13 live legs + `full_preflight` run (no skips) and pass —
the "still run and pass on the configured hosts" claim checks out.
