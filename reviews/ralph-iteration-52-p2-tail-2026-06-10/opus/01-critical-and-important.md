# Critical & Important Findings

## Critical

None. All quality gates pass; determinism holds; the committed cpuid artifact
still matches reality; no parser rejects the new log kind.

---

## Important

### I-1 (4ld, CORE): encoder fingerprint probe covers only 3 of 14 drained payload variants

This is the central correctness question for bead 4ld, and the current probe set
is too narrow to do the job the bead defines.

**The drained set.** `sdk_event_digest()`
(`crates/dh-devices/src/detchannel.rs:689`) computes the AUX SDK_EVENT digest by
re-encoding a drained `OwnedPayload` through `wire_view()`
(`detchannel.rs:540`). `wire_view` can produce **14** distinct `EventPayload`
variants, each routed to a distinct arm of `encode_event`
(`../guest-sdk/crates/detguest-wire/src/events.rs:283`):

1. Hello, 2. NameIntern, 3. AssertViolation, 4. Reachable, 5. Beacon,
6. InjectQuery, 7. RegionRegister, 8. RegionUpdate, 9. WorkloadStarted,
10. WorkloadExited, 11. LogLine, 12. QuiesceReady, 13. FrameMark, 14. Ready.

**The probe set.** `wire_encoder_fingerprint()` (`detchannel.rs:666`) digests
only `Pad`, `Hello`, `NameIntern`, `Beacon`. `Pad` is never a drained payload
(it is ring filler, not an `OwnedPayload` `wire_view` emits), so the probe
exercises just **3 of the 14** encoder arms that actually feed SDK digests.

**The consequence — the exact false-negative the bead exists to kill.** The
fingerprint's whole purpose (per its own doc-comment and the dhilog comment at
`crates/dh-inputlog/src/dhilog.rs:237`) is: a verifier replaying an old log with
a HEAD-wins-skewed encoder compares fingerprints *instead of* chasing spurious
SDK-digest divergence. But a wire-format change to, e.g., `RegionRegister`'s
4xu32 layout, `AssertViolation`'s length-prefixed `details`, or `Ready`'s
`manifest_generation` width would:
  - **skew** the SDK_EVENT digests for those payloads (they re-encode through the
    changed arm), yet
  - **NOT flip** the fingerprint (those arms are not in the probe set).

The verifier would then see a fingerprint MATCH and conclude "same encoder,
trust the digests" — and then flag a real-looking divergence on a digest that
actually changed only because the encoder changed. That is precisely the
spurious-divergence chase the fingerprint promises to prevent, except now it is
*masked* by a falsely-reassuring fingerprint, which is worse than no fingerprint.

**Why it is Important, not Critical.** No replay/verifier consumes the
fingerprint yet (write-only stage; no DHILOG reader exists in-tree — confirmed by
grep: only test assertions reference KIND_* on the read side). So nothing is
broken at runtime today. But the artifact the bead produces does not yet meet the
bead's own contract, and the fix is cheap, so it should land before any verifier
is built against it.

**Recommended fix (cheap, deterministic).** Extend the probe array to one
instance of every `EventPayload` variant `wire_view` can emit (all 14), each with
fixed literal fields. All are deterministically constructible:
`RegionRegister`/`RegionUpdate` take a `RegionEvent { region_id, name_id,
layout_version, manifest_generation }` of four u32s; `AssertViolation`/`LogLine`
take a fixed `&[u8]`; the rest are fixed scalars. Also exercise the
`extra_flags`/`FLAG_REACHABLE_DECL` and `FLAG_TRUNCATED` paths if you want the
fingerprint to bind header-flag semantics too (a `NameIntern` with
`reachable_decl` and an over-`MAX_DETAILS` `AssertViolation`). Keeping `Pad` is
fine (it binds the header/pad framing) — just additionally cover the 11 missing
drained variants.

Alternative (weaker, not recommended): document in the function doc-comment WHY a
3-variant subset is claimed sufficient. I could not construct such an argument —
the missing arms have independent layouts (length-prefixes, struct packing,
signed fields) that the 3 covered arms do not exercise — so extension is the
right call rather than documentation.

**Action:** file a bead to extend the probe set to all `wire_view` variants
before any verifier consumes KIND_ENCODER_FP. See `04-action-items.md` I-1.

### I-2 (nq5/8jx, latent): validate() rejects same-(function,index) duplicates that KVM can legitimately emit

`MachineConfig::validate` (`crates/dh-vmm/src/config.rs:167`) enforces a STRICT
ordering on the cpuid table:

```rust
.windows(2).all(|w| (w[0].function, w[0].index) < (w[1].function, w[1].index))
```

Strict `<` means two entries with the SAME `(function, index)` but different
`flags` are rejected as `CpuidTableUnsorted`. Meanwhile `to_leaves`
(`crates/dh-vmm/src/cpuid.rs:167`) sorts only by `(function, index)`
(`sort_by_key`, stable — good), so such a pair would survive into the table as
adjacent equal-key entries and trip validate.

**Today this does not fire.** I wired `to_leaves(masked_cpuid())` into a
`MachineConfig` and ran `validate()` on this host: 40 leaves, **0** duplicate
`(function,index)` pairs, validate **PASSES**. The 16 SIGNIFICANT_INDEX
(`flags=0x1`) entries are all subleaf families (fn 0x4 idx 0..4, fn 0xd idx 0..4,
etc.) with DISTINCT indices, so no collision.

**Why it is still worth flagging.** KVM's `KVM_GET_SUPPORTED_CPUID` is permitted
to return two entries with the same `(function, index)` and different flags (e.g.
a terminator subleaf 0 alongside a SIGNIFICANT_INDEX subleaf 0) on other
microarchitectures / kernel versions. When 8jx wires `to_leaves(masked)` into the
real boot `MachineConfig` and `validate()` runs as part of `canonical_encode`,
such a host would fail config validation with a misleading `CpuidTableUnsorted`.
The sort key and the validate key being `(function, index)` while the *encoding*
and the *uniqueness intent* really key on `(function, index, flags)` is the seam.

**Why Important, not Critical.** Not reachable on this lab box today and 8jx
hasn't wired the path yet. But it is a real cross-host landmine that should be
decided deliberately, not discovered when a different runner trips it.

**Options for 8jx to weigh (decide, don't necessarily fix now):**
  - Make the sort and the validate key both `(function, index, flags)` (allows
    legitimate same-(fn,idx) different-flags pairs; preserves determinism).
  - Or: collapse/canonicalize KVM's duplicate-key entries before `to_leaves`
    (if such pairs are never semantically meaningful for the masked set).
  - Whichever is chosen, add a regression test that feeds a synthetic
    same-(fn,idx)-different-flags table through `to_leaves` + `validate`.

**Action:** annotate bead 8jx with this constraint. See `04-action-items.md` I-2.
