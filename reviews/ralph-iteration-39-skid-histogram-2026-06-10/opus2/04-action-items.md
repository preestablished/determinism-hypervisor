# Action items

### Critical

_None._

### Important

- [ ] **Cross-reference the iter-16 "exactly 18" note with the iter-39 27..31
  result** so the empirics trail is not self-contradictory. Edit
  `crates/dh-vmm/src/run.rs:19-21`: scope the "18" claim to the real-mode
  single-instruction spin (the delivery-latency floor) and add that richer guests
  (stores + dependent chains, e.g. the landing_loop) skid ~10 more — still
  ≪ margin/2. 2-line doc change, no code. (See `01-critical-and-important.md` I-1.)
  Verified: 18 reproduced 6/6 on real-mode spin; 27..31 reproduced on landing_loop.

### Suggestions

- [ ] **`SkidReport.gate: Result<(), MarginViolation>`** instead of
  `Result<(), String>` — `tools/dh-cli/src/skid.rs:21,76-78`. Stop stringifying
  the typed error at the report boundary so programmatic consumers keep the
  structured `max_skid`/`skid_margin`. `main.rs` can call `Display` where it
  already does `eprintln!("{e}")`. (S-1)

- [ ] **Fix the empty-histogram alert text** — `crates/dh-verify/src/skid.rs:67-70`
  emits `max skid 18446744073709551615` for the no-data case. Use a `NoData`
  variant or `Option<u64>` so the message reads "no samples collected" instead of
  a fake 1.8e19. Behavior (gate fails) is already correct; this is cosmetic.
  Repro: `dh-cli skid --samples 0`. (S-2)

- [ ] **Make malformed `--samples` a usage error**, not a silent fallback to 200 —
  `tools/dh-cli/src/main.rs:252-256`. Distinguish "absent → default" from
  "present-but-unparseable → error" so `--samples 100O` doesn't quietly run 200.
  (S-3)

- [ ] **Optionally note the phase-locked tri-modal shape** (27/30/31, ~1/3 each)
  near the histogram docs, so a reader understands re-runs give identical buckets
  by design, not under-sampling. (S-4)

---

**Reviewer disposition:** APPROVE / ship. The one Important item is a 2-line doc
edit and does not block merge; the four Suggestions are polish. Live gate, full
suite, and clippy all green; measurement is deterministic on a busy host.
