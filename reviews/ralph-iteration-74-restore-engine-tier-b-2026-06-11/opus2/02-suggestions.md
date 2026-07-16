# Suggestions (non-blocking)

## S1 — Two pv-entropy devices on one bus slip past the shape check

**File:** `crates/dh-worker/src/restore_engine.rs:215-255`

The device loop sets `entropy_device_seen = true` and `continue`s for *each*
entropy device without incrementing `device_sections_consumed`. The container,
by contrast, carries exactly one `ENTR` section (the codec rejects duplicate
tags — `dhsnap.rs:247`). If a misconfigured bus had two `0x0004` devices, both
would call `dev.restore(entr.device_regs(), 1)` against the *same* ENTR blob,
and the `total_sections == 5 + device_sections_consumed` check (line 249) would
still pass because neither entropy device is counted. The symmetric hazard
exists on the capture side (`snapshot_engine.rs:249-251`: `entropy_regs = Some(...)`,
last-writer-wins, no duplicate detection). Neither side is wrong on a
well-formed single-entropy bus, but a `entropy_device_count == 1` assertion
(reject 0 *and* >1) would make the invariant explicit rather than emergent.
Low severity — the bus does not prevent the misconfiguration, but no caller
builds one.

## S2 — `as_any_mut` returning `Option<&mut dyn Any>` is unusual; consider documenting why over the conventional form

**Files:** `crates/dh-devices/src/lib.rs:64-66`, `clock.rs:181-183`,
`restore_engine.rs:258-268`

The conventional downcast seam is `fn as_any_mut(&mut self) -> &mut dyn Any`
with a blanket `Some(self)`-style default. Here it returns `Option`, default
`None`, and only `PvClock` overrides to `Some(self)`. This is a deliberate
"opt-in" design (a device that does not need engine-supplied state stays
`None`), and there is no *correctness* hazard: `downcast_mut::<PvClock>()`
returns `None` for any other concrete type, so a second device falsely
claiming `DEVICE_ID_PV_CLOCK` and returning `Some(self)` would fail the
`downcast_mut` and produce the loud `Codec("clock device does not downcast to
PvClock")` error (line 263-265) rather than silently corrupting state — good.
The footgun is purely conceptual: a future device author may override
`as_any_mut` to `Some(self)` "to be safe" even though it carries no
engine-supplied state, and the engine would never call it. A one-line note on
the trait method — "override ONLY if the restore engine must reach your
concrete type; the seam is keyed by `device_id`, so the override and the
id-match in the engine must agree" — would prevent that. Style-adjacent, keep
the `Option` form.

## S3 — Test blind spots worth a follow-up bead

**File:** `crates/dh-worker/tests/restore_engine.rs`

The mis-shape suite is thorough, but a few high-value negatives are untested:
- **ENTR v1-only rejection.** The doc and `EntrSectionV2::decode` reject a v1
  ENTR (`BadVersion`), but no test crafts a v1 ENTR section and asserts the
  `Codec("ENTR (engine requires v2)")` path. (The "missing ENTR" test exercises
  absence, not wrong-version.)
- **Duplicate-section container.** The codec rejects it (`dhsnap.rs:247`), so
  `Container::parse` already guards the engine — but a test that pushes two
  `TIME` sections via a raw builder and asserts `Codec("DHSNAP: ...")` would
  pin that the engine relies on parse-time uniqueness, not its own dedup.
- **A wrong device section length** (e.g. a 5-byte CLKD) to exercise the
  `device {id} rejected its section` branch (line 232-238) — currently only
  *missing* and *extra* sections are tested, not *malformed-but-present*.
- **vCPU section decode failure** (truncated VCPU) to exercise line 272-273.

## S4 — `pages_loaded` always reports total RAM even for a delta restore

**File:** `crates/dh-worker/src/restore_engine.rs:287, 362`

`RestoreOutcome.pages_loaded = total_pages` regardless of how many pages the
server actually streamed (a delta restore still materializes the full image).
This is correct for "pages written into the slot" but a reader expecting
"pages received over the wire" (the mirror of `take_snapshot`'s
`pages_shipped == dirty_count`) could be surprised. Either rename to
`pages_materialized` or document that it is always the full page count. Cosmetic.

## S5 — The `5` magic constant could be a named constant tied to the fixed sections

**File:** `crates/dh-worker/src/restore_engine.rs:249, 324`

`5 + device_sections_consumed` appears twice and the `5` (MCFG, VCPU, LAPC,
TIME, ENTR) is implicit. A `const FIXED_ENGINE_SECTIONS: usize = 5;` with a
comment listing the five tags would make the long-term coupling between the
capture layout (`snapshot_engine::build_dhsnap`) and this check explicit, so a
future section addition flags both sides. Maintainability nicety.
