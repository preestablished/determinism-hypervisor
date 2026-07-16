# Tests

Ran:

```bash
cargo test -p dh-worker capture -- --nocapture
```

Result: passed.

Covered by existing checkpoint tests:
- `run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer`
- `take_snapshot_capture_checks_layout_version_and_returns_features`

Important gaps:
- No VerifyReplay/RPC test records and replays a DetChannel/capture-fixture log.
- No `Run` test for `layout_version` mismatch or other failed-capture status after the guest has actually executed.
- No capture-neutrality acceptance test comparing capture vs no-capture state hashes/snapshot refs/epoch hashes, despite M6 requiring C5 coverage.
- No bound/abuse test for very large manifest-declared framebuffer regions or oversized range lists.
