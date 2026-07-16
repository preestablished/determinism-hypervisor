# Action Items

Self-contained checklist. File these against `crates/dh-vmm/src/hash.rs` unless noted.
"Preimage change" items should land **before** any `dh-statehash-v1` artifact is treated as a
baseline, while the version string is still free to keep.

### Critical

_None._

### Important

- [ ] **AI-1 — Align IA32_TSC to §8.1 preimage order.** Move the normalized TSC slot
  (`0x10 || le64(vns)`) to sit **between TSC_AUX and SPEC_CTRL**, matching ARCHITECTURE.md:643–646
  (`… PAT, TSC_AUX, IA32_TSC, SPEC_CTRL`). Today the impl appends it *after* SPEC_CTRL
  (hash.rs:294–301), transposing the two. Self-consistent now, but the M4 DHSNAP codec will
  serialize in §8.1 order and the preimages will disagree. Preimage change — do it pre-release.
  Add a comment binding the order to §8.1 and a golden-vector test (AI-6).

- [ ] **AI-2 — Add `le64` length prefixes around the variable-length link fields.** In both
  `push_link` (hash.rs:93–96) and `push_final_link` (hash.rs:124–126), prefix `vcpu_blob` and
  `device_sections` with `le64(len)`. Without delimiters the boundary between them is ambiguous:
  moving trailing blob bytes into the sections start yields an identical preimage. The codebase
  already uses this discipline (`config.rs` `length_prefixes_prevent_ambiguity`; per-device
  framing in `device_sections`). Preimage change — do it pre-release; also make framing
  normative in §8.5.

- [ ] **AI-3 — Resolve `KVM_GET_MSRS` partial-return policy.** The strict `n != len` check
  (hash.rs:287–293) hard-fails `push_final_link` on any host where a listed MSR (notably
  SPEC_CTRL/0x48 on pre-IBRS CPUs) is unreadable — `get_msrs` stops at the first unreadable
  index and returns the count read. Preferred fix: gate required-MSR readability at slot
  creation / preflight (fail fast there), so by hash time the set is known-readable, keeping the
  strict check but off the hot path. Alternative: define a deterministic, machine-config-bound
  preimage for an absent MSR so two hosts with different availability never compare equal by
  accident. Add a comment explaining the partial-return semantics either way.

### Suggestions

- [ ] **AI-4 — Cover `exception_has_payload` / `exception_payload`.** `kvm_vcpu_events` carries
  these two trailing fields (kvm-bindings 0.14 x86_64 bindings.rs:1736–1737); the blob omits
  them while serializing `events.flags`. Either serialize both unconditionally (9 bytes) or add
  an assert that `flags & KVM_VCPUEVENT_VALID_PAYLOAD == 0` plus a doc note. Prefer serializing.

- [ ] **AI-5 — Pin KVM struct sizes.** Add `const _` size assertions (or a unit test) on
  `kvm_fpu` / `kvm_segment` / `kvm_vcpu_events` so a future kvm-bindings bump that grows a
  struct forces a revisit of the hand-written serializers instead of silently dropping a field.

- [ ] **AI-6 — Add a golden-vector test.** Feed fixed inputs through the chain and assert a
  hardcoded 32-byte hash, locking the exact preimage layout. Land it together with AI-1/AI-2 so
  the vector encodes the corrected order/framing. This converts every future preimage drift into
  a red test.

- [ ] **AI-7 — Guard the `device_sections` length cast.** `section.len() as u32` (hash.rs:315)
  truncates silently above 4 GiB; add a `debug_assert!(section.len() <= u32::MAX as usize)`.

- [ ] **AI-8 — (optional) Share the link head/tail hashing** between `push_link` and
  `push_final_link` via a small helper so the two preimage definitions cannot drift. Current
  duplication is small and readable; low priority.

### Version-discipline note (cross-cutting)

If AI-1 and/or AI-2 land **before** release, no version bump is needed — `dh-statehash-v1`
remains valid because no consumer has baselined an artifact yet. If for any reason these
preimage changes (or a future SREGS→SREGS2 upgrade at M4) land **after** any state hash is
exchanged or persisted, the domain-separation string **must** bump (`dh-statehash-v2`) so old
and new hashes can never be silently compared as equal. Recommend deciding the cutover now:
fix the preimage at v1 pre-release, and reserve a v2 bump for the M4 SREGS2/XSAVE upgrade,
which inevitably changes the preimage regardless.
