# Action Items

Self-contained checklist for `ralph/iteration-71-entr-section-round-trip` (bead 6yl).
Verdict: **NEEDS_DISCUSSION** — code is correct and tested; the open items are spec/golden
hygiene around introducing a new on-disk format version.

### Critical

- [ ] None.

### Important

- [ ] **Ratify and record the ENTR v2 spec divergence.** Writing a 72-byte
      `sec_version = 2` ENTR section diverges from API.md §4
      (`.agents/docs/determinism-hypervisor/API.md:618`), which says ENTR is "exactly …
      (56 bytes)" with no v2 row. Today it is documented only in the `dhsnap.rs:363-366`
      struct comment. Do ONE of:
  - [ ] **(Preferred)** Edit API.md §4:618 to make ENTR explicitly versioned — v1 (56 B)
        = PRNG state; v2 (72 B) = PRNG state ‖ pv-entropy regs `{buf_gpa u64, len u32,
        status u32}`; note the engine writes v2 and readers decode by `sec_version`. The
        section header already carries `sec_version: u16` (§4:607), so this makes the
        producer conformant rather than divergent. (Suggested row text is in
        `01-critical-and-important.md` I1.)
  - [ ] OR, if §4 is owned upstream and frozen here, record the divergence in the
        project's durable spec-delta ledger (the prompt calls it "veu divergence #7").
        NOTE: no such ledger / no `veu` bead currently exists in this tree — if it does
        not exist, file a bead to create it and link 6yl. A struct doc comment is not a
        durable record.

- [ ] **Add a frozen golden-bytes fixture for ENTR v2.** API.md §4:25-26 requires a
      checked-in fixture "per version" that parses to a pinned debug repr and
      re-serializes byte-identically. `golden.rs` pins only v1 ENTR
      (`golden.rs:73-77`, `:175-180`); the v2 layout is only self-round-tripped at runtime
      in `entr_roundtrip.rs`, so a field-order regression in v2 would still pass. Add a
      72-byte checked-in v2 fixture to `golden.rs` mirroring the v1 ENTR block (parse +
      byte-identical re-encode; a `blake3` container pin matches the kitchen-sink pattern).

### Suggestions

- [ ] Add an ENTR version-dispatch helper (`decode_any` / `enum EntrAny`) when the FIRST
      real engine consumer of ENTR lands — not speculatively. No consumer exists today.
      (S1)
- [ ] Consider having `EntrSectionV2::from_parts` take structured reg fields instead of a
      `&[u8]` to remove the second copy of the device-reg byte layout — only if a real
      (non-dev) `dh-snapshot → dh-devices` dependency is acceptable. (S2)
- [ ] If a multi-stream design ever lands, add a live golden case where `stream` is
      non-zero through the full restore path; today `from_seed` never sets `stream`, so
      only the non-live test pins a non-zero value. (S3)
- [ ] `Default` derive on `EntrSectionV2` is dead surface (zero seed/word_pos is not a
      meaningful state); keep for symmetry with v1 or drop when trimming API. (S4)

---

**Test status:** `cargo test -p dh-snapshot` PASSES (18 unit + 2 entr_roundtrip + 4
golden + 1 readiness, all green). No regressions; v1 decode unchanged.
