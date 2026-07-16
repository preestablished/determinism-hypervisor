# Action Items

### Critical

(none)

### Important

(none)

### Suggestions

All optional; verdict is APPROVE regardless. Each is self-contained.

- [ ] **S1 — Disambiguate the Row 1 hash.** In `docs/phase-1-exit-gate.md` Row 1,
  the cited state hash `7e09ac13…` is the TIMER sub-gate fingerprint; the plain
  sub-gate fingerprint is `482edfed…`. Either label `7e09ac13…` explicitly as the
  timer sub-gate hash or list both, so the value isn't mistaken for a single
  canonical "Phase-1 hash". (Both reproduced live; this is wording only.)

- [ ] **S2 — Soften the Row 5 wall-clock numbers.** "timer … 95.7 s" and
  "regression … 5.5 s" are run-specific and drift (observed 91–95 s and ~3.9 s on
  re-run). Either drop the seconds (they don't bear on determinism) or annotate
  them as indicative-this-run. File `docs/phase-1-exit-gate.md`, Row 5.

- [ ] **S3 — Mention the 10M companion regression test (optional).** Row 5 cites
  only "regression (1e9 ×2)"; the suite also contains
  `ten_million_twice_equal_final_hash` (green). Noting both avoids a reader
  finding an unmentioned passing test. Cosmetic only.
