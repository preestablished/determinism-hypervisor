# Suggestions (non-blocking)

These are quality nits. None block merge; none weaken the strict checks.

## S1 — The shape-count check runs after device state is already mutated

`restore_engine.rs:212-255`. The reverse shape check
(`total_sections != 5 + device_sections_consumed`) fires only after the
device-restore loop has already called `dev.restore(...)` on every matched device.
An over-shaped container (extra foreign section) is therefore detected only once
some device state has been written. This is *fine* under the documented scrap
contract (the slot is discarded on any error), but the error could be raised
slightly earlier and more cheaply: the section count is knowable from `dhsnap`
immediately after parse, and the expected device count is knowable from the bus
before the loop. Consider computing the expected non-entropy device count up front
(one `bus.devices().filter(...).count()` pass) and validating
`total_sections == 5 + expected` *before* the mutation loop. This is purely a
defense-in-depth / clarity improvement — behavior is unchanged.

## S2 — `set_vns_base` loop re-walks the bus a second time

`restore_engine.rs:257-268`. The PvClock `vns_base` is set in a separate
`bus.devices_mut()` pass, after the device-restore loop. It reads cleanly, but it
means two full bus walks. An alternative is to fold the `set_vns_base` into the
device-restore loop (when `id == DEVICE_ID_PV_CLOCK`, do the `restore` then the
downcast + `set_vns_base` in the same arm). The current two-pass form is arguably
*more* readable because it keeps the §8.3 "device restore" and "clock re-seed"
steps visually distinct, so this is genuinely optional — flagging only because the
double `as_any_mut`/downcast path is the one slightly unusual mechanism in the
file and consolidating it would make the seam appear exactly once.

## S3 — `device.restore(...)` error detail is dropped for the entropy device

`restore_engine.rs:220-221`. For the pv-entropy reg restore, the underlying
`RestoreError` is mapped with `.map_err(|_| ...)` to a fixed string, discarding the
device's own error. The non-entropy arm (`:232-238`) does the same but at least
interpolates the section version and length into the message. Since `RestoreError`
is a unit struct (`dh-devices/src/lib.rs:30`) there is little detail to preserve,
but for symmetry/diagnosability you could include the ENTR `device_regs()` length
(always 16) or the `entr` version in the entropy-arm message too. Very minor.

## S4 — `pages_loaded` is `total_pages`, not the count actually resolved

`restore_engine.rs:286-287` reports `pages_loaded: total_pages`. Because the
coverage check guarantees every page was written, this equals the number resolved,
so it is not wrong — but the field name suggests "pages I loaded from the store,"
whereas a flattened FULL set always equals total RAM pages. If a caller ever wants
to distinguish "store sent N entries" from "RAM has M pages" (e.g. metrics on
chain depth / redundant writes), `resolved.len()` would be the more informative
value. Consider either renaming the doc comment to clarify it is "total guest RAM
pages materialized" or returning `resolved.len()`. Cosmetic.

## S5 — Consider a golden-bytes assertion alongside the ref equality

`tests/restore_engine.rs:310-313` and `:469`. The transparency tests assert
`resnap.snapshot_ref == snap.snapshot_ref`, which is the right end-to-end property
(the ref is `blake3(container)`, so ref-equality *is* byte-equality of the
container, per `client.rs:846-875`). The rust-nostd-wire-codecs research note
recommends pinning actual bytes, not just round-trips, to catch wrong-but-symmetric
layouts — but here the ref already comes from the store's content hash, so the byte
identity is fully exercised. No change strictly needed; if you ever want the test
to localize a regression to the container vs the page set, you could additionally
assert the resolved page sets are byte-equal. Optional.

## S6 — `MEM` not multiple-of-PAGE_SIZE is an unstated invariant

`restore_engine.rs:133` computes `total_pages = slot.mem_bytes / PAGE_SIZE` with
truncating division (mirrors `snapshot_engine.rs:121`). If a slot were ever created
with a non-page-multiple `mem_bytes`, the tail bytes would be silently excluded
from both capture and restore. This is a pre-existing slot-creation invariant, not
introduced here, but a one-line debug-assert or doc note at the engine boundary
(`mem_bytes % PAGE_SIZE == 0`) would make the assumption explicit. Optional.
