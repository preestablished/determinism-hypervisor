# Action Items

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] **S1 (optional parity):** In `crates/dh-snapshot/tests/golden.rs`, positively assert
  the on-disk header version equals `FORMAT_VERSION` (0x0100) — e.g. read `fixture[6..8]`
  as a `u16` LE in `minimal_fixture_parses_empty` / `kitchen_sink_fixture_parses_to_pinned_sections`.
  Closes the small gap vs. DHILOG's `h.version == FORMAT_VERSION` assertion. Currently only
  guarded indirectly (parse rejects bad versions) and via the frozen-bytes hash.
- [ ] **S2 (optional uniformity):** If every engine field should be independently
  byte-order-discriminating, give `TimeSection.epoch_index` an ascending-distinct value
  (e.g. `0x0708090A0B0C0D0E`) instead of `20`. MUST be done as a format-version bump into
  NEW fixture file names with freshly computed hashes — never edited in place on the v1.0
  files. Immaterial today since `icount`/`vns` already cover the u64 LE layout.
- [ ] **S3 (optional self-description):** Assert `sec_version == 1` on the device-shaped
  sections in the parse test so the per-section version is an explicit part of the reader
  freeze (currently only pinned implicitly via the byte hash and the TIME/ENTR typed
  decode). One assertion or a small loop.

## Verification log (for the record)

- [x] `cargo test -p dh-snapshot --test golden` — 4/4 pass
- [x] `cargo test -p dh-snapshot` (full suite) — 23/23 pass (18 unit + 4 golden + 1 readiness)
- [x] Independent BLAKE3 recompute of both fixtures — both match pinned constants
- [x] Independent pure-Python byte reconstruction from spec — both fixtures byte-identical
- [x] Reader-half coverage — 11/11 §4 sections pinned + tag-order vs `KNOWN_TAGS`
- [x] Byte-order sensitivity of engine constants — confirmed ascending-distinct, half-swap
      detectable on `word_pos`
- [x] Freeze-scope doc accuracy — consistent with `dhsnap.rs` ownership split + iter-64 review
- [x] `*.dhsnap binary` gitattributes + `blake3.workspace` dep wiring — correct
