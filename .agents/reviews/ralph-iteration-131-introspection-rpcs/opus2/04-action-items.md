# Critical

- [ ] No critical action items.

# Important

- [ ] Fix `GetFramebuffer` so non-empty `pixels` are accompanied by metadata where `pixels.len() == stride * height`, or explicitly fail raw framebuffer regions until a framebuffer metadata descriptor exists.

- [ ] Move paused-slot validation to the serialized actor execution point, or add a per-slot introspection/read reservation so `Run` cannot overtake an already accepted paused-boundary introspection request.

# Suggestions

- [ ] Add a bounded retained-event memory policy for `SlotRuntime.guest_events`, with an explicit `RESOURCE_EXHAUSTED` or loss policy when the cap is exceeded.

- [ ] Document `StreamGuestEvents` as at-most-once if selected events should be consumed before the tonic stream is fully read, and add tests covering filtered retention plus cancellation/drop behavior.

- [ ] Convert the requested `streams` filter to a set before draining `runtime.guest_events` so duplicate filters are harmless and filtering is not O(events * filters).
