# Residual Risks

- DetChannel replay semantics are subtle because recording creates some canonical records as a side effect of servicing PIO exits. Replay needs to regenerate or verify those records consistently, not simply run to their icount and apply them blindly.
- `capture_at_boundary` allocates `feature_bytes` and full framebuffer buffers from request/guest-manifest lengths. A bad manifest or oversized request can drive large worker allocations unless a higher layer already enforces limits.
- `CaptureSpec.framebuffer` selects the first live manifest entry with the framebuffer flag. If guests publish multiple framebuffer-flagged regions, the result depends on manifest entry order.
- The framebuffer output uses `lz4_flex::compress_prepend_size`; clients must agree that `fb_lz4` is size-prepended LZ4 block data, since the proto comment only says "lz4-compressed framebuffer pixels".
