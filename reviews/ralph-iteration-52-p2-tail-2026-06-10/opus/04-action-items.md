# Action Items

Verdict: **APPROVE**. Nothing blocks merge of this iteration. The items below are
follow-ups (one Important bead, two suggestions). All are self-contained.

### Critical

None.

### Important

- [ ] **I-1 — Extend the 4ld encoder-fingerprint probe to every drained payload
  variant.** File a bead (block any future "verifier consumes KIND_ENCODER_FP"
  work on it). In `crates/dh-devices/src/detchannel.rs`,
  `wire_encoder_fingerprint()` currently digests only `Pad`, `Hello`,
  `NameIntern`, `Beacon` (3 drained variants of 14). Add one deterministic
  instance of every `EventPayload` variant that `wire_view`
  (`detchannel.rs:540`) can emit: AssertViolation, Reachable, InjectQuery,
  RegionRegister, RegionUpdate, WorkloadStarted, WorkloadExited, LogLine,
  QuiesceReady, FrameMark, Ready. All are constructible with fixed literals
  (`RegionEvent` is four u32s; AssertViolation/LogLine take a fixed `&[u8]`).
  Optionally also cover the header-flag paths (`FLAG_REACHABLE_DECL` via a
  `reachable_decl` NameIntern; `FLAG_TRUNCATED` via an over-`MAX_DETAILS`
  AssertViolation). Then pin the resulting fingerprint as a golden u64 (see S-3).
  Rationale: a wire-format change to any uncovered arm skews SDK digests without
  flipping the fingerprint — the exact false-negative the bead exists to prevent,
  and worse than no fingerprint because it falsely reassures the verifier.

- [ ] **I-2 — Annotate bead 8jx: same-(function,index) different-flags collision
  vs validate().** `MachineConfig::validate` (`crates/dh-vmm/src/config.rs:167`)
  uses strict `(function,index) < (function,index)`, which rejects two leaves
  sharing `(function,index)` but differing in `flags`. `to_leaves`
  (`crates/dh-vmm/src/cpuid.rs:167`) sorts only by `(function,index)`. On this
  lab box the real masked table has 0 such collisions and validate passes, but
  KVM can legitimately return such pairs on other hosts/kernels. When 8jx wires
  `to_leaves(masked)` into the boot config (validate runs inside
  `canonical_encode`), a different runner could fail with a misleading
  `CpuidTableUnsorted`. Decide: either key sort+validate on
  `(function,index,flags)`, or canonicalize KVM's duplicate-key entries before
  `to_leaves`. Add a regression test feeding a synthetic same-(fn,idx)
  different-flags table through `to_leaves` + `validate`.

### Suggestions

- [ ] **S-1 — Emit the fingerprint at SEGMENT START (M4 replay design).**
  Emission is tied to the live attach (`PORT_INIT_GO`), not segment start;
  `restore()` re-attaches without a log handle and emits nothing, so a
  post-restore segment carries SDK_EVENT digests but no fingerprint to gate them.
  Carry into the M4 replay/segmenting bead: emit per segment, not (only) per
  attach.

- [ ] **S-2 — Consider hoisting the fingerprint into `SegmentHeader`.** It is a
  constant-per-build 8-byte value, i.e. segment metadata, not a timestamped
  event. Header placement makes it unconditionally present and fixed-offset for a
  minimal replayer. Bigger change (DHILOG-v1 layout = a compat event); raise when
  the replay format is finalized.

- [ ] **S-3 — Make the fingerprint test pin a golden value.**
  `encoder_fingerprint_is_deterministic_and_logged_at_attach`
  (`detchannel.rs:859`) only checks self-equality (trivially true). Assert
  against a frozen golden u64 so an accidental encoder change is caught. Also fix
  the test name's "...and_logged_at_attach" overpromise (the body doesn't
  exercise the attach emit path).

- [ ] **S-4 — One-word comment on the `* 28` capacity hint** in
  `cpuid_leaves_hash` (`config.rs:53`): `// 7 u32 fields`. Value is correct; just
  prevents a future reader recomputing it.
