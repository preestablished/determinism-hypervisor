# Positive Notes

## P1 — Capture/restore byte symmetry is genuinely tight

Every `DetDevice::restore` on the bus strictly validates *both* `sec_version`
and exact byte length and returns `RestoreError` on any mismatch (verified:
`clock.rs:170-177`, `entropy.rs` 16-byte, `pad.rs` 24-byte, `serial.rs`
empty-only, `blk.rs:271-306` with a per-cluster blake3 digest re-check). The
restore engine pairs each device to its section by tag via the single-source
`tag_for_device_id` map (`restore_engine.rs:225`), so restore order is
irrelevant to correctness — the fixed-point property cannot be broken by bus
iteration order. This is the kind of by-construction symmetry that makes the
take→restore→take ≡ ident-ref test (`tests/restore_engine.rs:627-693`)
trustworthy rather than coincidental.

## P2 — The shape check is robust against duplicates by leaning on the codec

`restore_engine.rs:248-255`'s `total_sections == 5 + device_sections_consumed`
looks fragile but is not, because `Container::parse` rejects duplicate tags
(`dhsnap.rs:247`) and `get` returns the unique section. The "two clocks / two
of any tag" edge case is therefore impossible to construct through the normal
path, and the reverse-direction count (every container section must find a
consumer) closes the both-ways shape gate cleanly. The entropy device is
correctly accounted as one of the *fixed five* (ENTR), not a consumed device
section — the `continue` without incrementing the counter is exactly right.

## P3 — The ENTR version-domain split is handled correctly

`restore_engine.rs:220` feeds `dev.restore(&entr.device_regs(), 1)` — the
DEVICE's version 1, never the ENTR section's 2 — matching the documented
`6yl` landmine (`dhsnap.rs:407-411`) and PvEntropy's strict
`sec_version != 1 => RestoreError`. The PRNG half is reconstructed separately
via `DetEntropy::restore(EntropyState{...})` (line 292-296), and the test
asserts the restored stream continues byte-for-byte from A's position
(`tests/restore_engine.rs:606-612`) — the actual property that makes the
re-snapshot a fixed point.

## P4 — Server-side flatten is correctly trusted and verified

The engine calls `resolve_pages(ref, None, false)` (line 134-135), and the
snapstore server's Mode-A handler flattens the full delta chain child-first,
enforcing full coverage (`FlattenError::Coverage` on any gap) and de-duplicating
by page index. The engine's own `covered[]` bitmap + out-of-range/duplicate/
short-payload/None-payload checks (lines 137-165) are a correct defense-in-depth
re-statement of that invariant, and ARCH §8.3 step 1 explicitly specifies
"flattened server-side into a full page list." The trust boundary is consistent
with the capture side (which also trusts the store's `batch_blake3` cross-check),
and the `get_snapshot` footer is BLAKE3-verified against the ref client-side.

## P5 — Honest, loud failure model

`RestoreError` variants carry precise context strings, the partial-write danger
is documented at the function level (`restore_engine.rs:88-91`: "On error the
slot's contents are UNDEFINED ... the caller must discard it"), and every
shape/version/identity mismatch is a hard error rather than a best-effort
restore. The MCFG identity check (line 182-190) refuses to guess across machine
shapes, and the LAPC empty-v1 expectation rejects a newer writer's lapic stub
rather than silently dropping interrupt state (`tests/restore_engine.rs:1043-1055`).

## P6 — `vns_base` clock seam is the right mechanism and is overflow-safe

Setting `PvClock.vns_base = time.vns` at segment-relative icount 0 keeps guest
time monotone across restore (`tests/restore_engine.rs:614-621` reads back
exactly `TIME.vns`). `vns()` uses `saturating_add` (`clock.rs:84-85`), so even
a `TIME.vns == u64::MAX` boundary cannot wrap — vns(0) is exactly the base, and
the existing `vns_saturates_at_u64_max` test already covers `set_vns_base(u64::MAX)`.
The downcast seam fails loudly if the clock id maps to a non-`PvClock` type
(line 263-265), so the seam cannot silently bind the wrong device.
