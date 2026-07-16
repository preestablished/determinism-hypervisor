# Action Items

## Critical

- [ ] Fix detchannel event icount domains so DHILOG records are always stamped with segment-relative icounts and `StreamGuestEvents` reports a consistent cumulative icount after restore/snapshot and across sequential runs.
- [ ] Add a regression test that restores or snapshots to a nonzero cumulative base, emits/drains detchannel events, streams them, and then successfully seals a DHILOG segment with ordered segment-relative records.

## Important

- [ ] Ensure any pause-boundary detchannel drain failure after a successful run marks both the runtime and the `SlotManager` slot as `Faulted` before returning an error.
- [ ] Decode the framebuffer descriptor from the `FRAMEBUFFER` region and return real `width`, `height`, `stride`, `format`, and exactly `stride * height` pixel bytes from `GetFramebuffer`.
- [ ] Add a descriptor-bearing framebuffer fixture/test so `GetFramebuffer` cannot pass with zero metadata or descriptor bytes included in `pixels`.

## Suggestions

- [ ] Replace `streams.contains(&event.stream)` in `StreamGuestEvents` with a request-local set to keep filtering predictable for larger event backlogs.
- [ ] Factor shared framebuffer lookup, descriptor decoding, size checks, and pixel extraction so `GetFramebuffer` and `CaptureSpec.framebuffer` use the same helper.
- [ ] Add stream retention tests showing that non-selected event streams remain buffered and are returned by a later `StreamGuestEvents` call.
