# Action Items

Self-contained items for `crates/dh-vmm/src/hash.rs` on branch `ralph/iteration-32-state-hash-chain`
(bead 35z). Each is independently actionable.

### Critical

_None._

### Important

- [ ] **Add `StateHashChain::from_value([u8;32]) -> Self`** so a restored/forked guest can resume the
  chain from the stored value (ARCH §8.1 stores the chain value; §8.3 restore step 3 restores the
  hashchain). Currently only `new()` (H_0) exists and `value` is private, so restore would be forced to
  drop the execution-history prefix or reach into a private field. Three lines; preserves the bead's
  "M4 extends, never replaces" contract. (I-1)

- [ ] **Test and wire `device_sections()`.** It is the only producer of the variable-length link
  region and has zero callers and zero tests. Add a unit test that builds a small `MmioBus` (two
  `DetDevice`s) and asserts `device_sections` output is deterministic and base-order-stable, plus one
  `push_link` test feeding non-empty device bytes. Confirms the `id||version||len||bytes` framing and
  prevents the dead code from rotting before M4 wires it in. (I-2)

### Suggestions

- [ ] **Length-prefix the variable preimage regions** while the format is `dh-statehash-v1`: emit
  `le64(vcpu_blob.len())` before the blob and `le64(device_sections.len())` before the device region
  (and/or a page count before the page loop). Makes the preimage self-delimiting so a future framing
  bug can't silently shift page boundaries. One-time format bump now vs. a v2 migration later. (S-1)

- [ ] **Reconcile MSR order with §8.1.** Code emits `... TSC_AUX, SPEC_CTRL, IA32_TSC(slot)`; §8.1 doc
  lists `... TSC_AUX, IA32_TSC, SPEC_CTRL`. Either reorder the code to match the doc, or add a one-line
  §8.1 note that the synthesized normalized-TSC slot is appended last by construction — so the M4
  DHSNAP codec and the hash cannot drift. (S-2)

- [ ] **Hash `events.exception.payload` (or document the boundary-quiescence assumption).** `flags` is
  already hashed, but two states with equal flags and differing `exception_payload` would collide.
  Phase-1-safe only because boundaries are quiescent — make the blob complete-by-construction (one
  `to_le_bytes`) or state the assumption in a comment. (S-3)

- [ ] **Comment the full-RAM walk as the Phase-1 stand-in** (`// M4 swaps dirty-ring delta + rayon
  fan-out, §8.2`) so the few-seconds serial walk over the max ~832K-page guest isn't mistaken for
  steady-state cost. (S-4)

- [ ] **Use a more specific `KvmError` variant** than `Open` for vCPU-capture / short-MSR-read
  failures (e.g. `Capture`/`Msr`). Cosmetic; improves error legibility. (S-5)

### Verification performed by this reviewer

- Ran `cargo test -p dh-vmm hash -- --nocapture` on this host (`/dev/kvm` rw): **7 passed, 0 failed,
  0 skipped**; both live KVM tests executed (grep for "skip" → 0).
- Confirmed `KVM_GET_MSRS` returns the full 14-entry list incl. SPEC_CTRL (implied by both live tests
  passing through the `n != 14` guard).
- Cross-checked the blob layout against ARCH §8.1 MSR list and §8.5 normative hash definition, and the
  `MmioBus::devices()` base-order contract (bus.rs:122) + `DetDevice` trait (lib.rs:41-56).
