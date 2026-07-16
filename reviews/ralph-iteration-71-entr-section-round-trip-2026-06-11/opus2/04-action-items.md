# Action Items

These are forward-looking follow-ups, not merge blockers for
`ralph/iteration-71-entr-section-round-trip`. The branch is APPROVED as-is.

## Action Items

- [ ] **(Important, I1 — file on qmp bead or a doc bead)** Document the ENTR-v2 → device
      version-mismatch seam at `crates/dh-snapshot/src/dhsnap.rs:407` (`device_regs`). The qmp
      restore engine MUST call `device.restore(&v2.device_regs(), device.section_version())`
      with the DEVICE's version (**1**) — NOT the ENTR section's version (**2**). Verified by
      experiment: `PvEntropy::restore(regs, 2)` returns `Err(RestoreError)`
      (`crates/dh-devices/src/entropy.rs:174-177`, `SECTION_VERSION = 1`). Add a `/// SEAM:`
      doc comment naming the two distinct version domains (snippet in 01-critical-and-important.md).

- [ ] **(Important, I2 — file a small testing bead)** Add a PRNG known-answer test to
      `crates/dh-devices/src/entropy.rs` `tests`. No KAT exists anywhere in `crates/` today
      (grep confirmed); the golden DHILOG/DHSNAP fixtures pin framing/digests but never raw
      ChaCha20 output, so a silent `rand_chacha` minor-version stream change would NOT fail
      loudly. Pin (generated on the lock-pinned `0.3.1`):
      - `seed=[0x42;32] stream=0` → `a4ddf31f7f32ba696f14ce50ecf3f21e3e100e83bdf47966e7b07468e9500b6e`
      - `seed=[0x42;32] stream=7` → `bfd3208145f94296daadaaa40c677ef89d75312e77d0bd23b115058d4e9d7e18`
      Full test in 01-critical-and-important.md (I2). Optionally pin
      `crates/dh-devices/Cargo.toml:11` to `=0.3.1`.

- [ ] **(Suggestion, S1)** Add one assertion covering `EntrSectionV2::decode`'s `BadVersion`
      branch with a correctly-sized (72-byte) buffer at a wrong version, e.g.
      `assert_eq!(EntrSectionV2::decode(&[0u8; EntrSectionV2::LEN], 3), Err(SectionError::BadVersion { found: 3 }))`.
      Brings v2 to parity with the v1/TIME `BadVersion` coverage in `dhsnap_codec.rs:318,326`.

- [ ] **(Docs, related to veu)** API.md §4 `ENTR` row
      (`.agents/docs/determinism-hypervisor/API.md:618`) still says *"exactly seed[32],
      stream u64, word_pos u128 (56 bytes)"* — which v2 diverges from (72 bytes when the
      engine writes it). The in-code RESOLVED note (dhsnap.rs:75-89) documents the v2 layout
      for engineers, and v1 stays the spec-exact 56-byte producer, so this is NOT a code
      defect. Decision needed: add a `veu` entry (the docs-divergence bead) for an §4 row
      noting "the snapshot engine emits sec_version 2 (72 bytes = PRNG ‖ pv-entropy regs); v1
      remains the spec-exact 56-byte layout." Recommendation: **yes, add a veu entry** — §4 is
      the on-wire contract and a 72-byte ENTR section on the wire is a real spec divergence
      that the in-code note alone does not surface to spec readers. The in-code note suffices
      for implementers but not for the API.md contract.

## Verification log (this review)

- [x] `cargo test -p dh-snapshot` — pass (entr_roundtrip 2/2, golden 4/4, codec, readiness)
- [x] `cargo test -p dh-devices entropy` — pass (7/7)
- [x] `cargo clippy -p dh-snapshot -p dh-devices --all-targets` — clean, 0 warnings
- [x] Scratch: device.restore version-2 rejection confirmed (REAL trap → I1)
- [x] Scratch: nonzero stream=7 continuation exact (committed test only covers stream=0)
- [x] Scratch: KAT vector generated on pinned rand_chacha 0.3.1 (→ I2)
- [x] Tree clean after scratch removed (`git status --porcelain` empty)
