# Review Overview — ENTR Section Round-Trip (EntrSectionV2)

- **Branch:** `ralph/iteration-71-entr-section-round-trip` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** 6yl — ENTR section: entropy `{seed, stream, word_pos}` round trip through DHSNAP (CLOSED)

## Summary

This iteration resolves the iteration-64 "two-payloads-one-tag" landmine by introducing
`EntrSectionV2` (sec_version 2, 72 bytes = 56B VMM-owned ChaCha20 PRNG state ‖ 16B
pv-entropy device MMIO regs `{buf_gpa, len, status}`) in
`crates/dh-snapshot/src/dhsnap.rs`, plus an integration round-trip test
(`crates/dh-snapshot/tests/entr_roundtrip.rs`) that wires the LIVE `DetEntropy` device
through the full container codec and proves bit-identical continuation. `dh-devices` is
added as a `dh-snapshot` **dev-dependency** only (no production coupling). The v1
56-byte `EntrSection` remains decodable for spec-exact producers.

The diff is small (210 insertions, 4 files), self-contained, and the LANDMINE comment at
`tag_for_device_id` is correctly downgraded to a RESOLVED note. The byte layout is
internally consistent and the device-regs half matches the live `PvEntropy::snapshot`
order exactly (verified by experiment).

## Verification performed (by experiment, not by mirroring reviewer 1)

- `cargo test -p dh-snapshot` — **pass** (entr_roundtrip 2/2, golden 4/4, codec, readiness).
- `cargo test -p dh-devices entropy` — **pass** (7/7).
- `cargo clippy -p dh-snapshot -p dh-devices --all-targets` — **clean**, zero warnings.
- **Scratch experiment 1 (seam):** constructed a `PvEntropy`, snapshotted its 16 regs,
  and confirmed `device.restore(regs, 2)` → `Err(RestoreError)` while
  `device.restore(regs, 1)` → `Ok`. The version-mismatch trap the prompt asks about is
  **REAL**: the engine MUST pass the DEVICE's version (1), not the ENTR section version
  (2), or restore fails. This seam is **not documented** at the `device_regs()` call site.
- **Scratch experiment 2 (nonzero stream):** the committed test only exercises stream=0.
  Re-ran with `stream=7` (set via `restore`) through burn + 37-byte sub-word fill →
  continuation exact across all probe sizes. Stream survives `state()`.
- **Scratch experiment 3 (KAT):** produced a known-answer vector (see 02-suggestions).
  `seed=[0x42;32] stream=0` → first 32 bytes `a4ddf31f7f32ba696f14ce50ecf3f21e3e100e83bdf47966e7b07468e9500b6e`.
- Tree clean after scratch removed (`git status --porcelain` empty).

## Verdict

**APPROVE.** The implementation is correct, tested end-to-end, clippy-clean, and the bead
is legitimately resolved. No Critical or blocking-Important findings. Two Important-but-
non-blocking gaps for **follow-up beads** (not this branch): (1) the unwritten v2-split
seam in the qmp engine has a version-mismatch trap that should be documented at
`device_regs()` before qmp is built; (2) `rand_chacha` is caret-ranged (`"0.3"`) with no
known-answer test — a silent minor-version stream change would not be caught loudly. Both
are forward-looking; the landed code is sound.

## Stats

| Metric | Value |
|---|---|
| Files changed | 4 (Cargo.lock, dh-snapshot/Cargo.toml, dhsnap.rs, +entr_roundtrip.rs) |
| Lines | +210 / −9 |
| Critical findings | 0 |
| Important findings | 2 (both follow-up beads, not blockers) |
| Suggestions | 3 |
| Tests added | 2 integration tests (entr_roundtrip.rs) |
| Test result | all pass; clippy clean |
