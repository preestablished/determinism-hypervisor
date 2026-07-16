# Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] [`crates/dh-snapshot/tests/snapstore_readiness.rs:9-12`] Fix the module doc comment:
      it claims the page channel "is selected via `Transport::Auto`'s `page_channel_path`
      field," but the sibling crate documents that field as reserved for the M5/WI3
      page-channel arm and **currently unused** (`Transport::connect` destructures it as
      `page_channel_path: _`). Reword to say the field is reserved/unused and the pin only
      locks its existence and type for the WI3 seam. Non-blocking (comment only); the code
      pin is correct.

### Suggestions
- [ ] [`crates/dh-snapshot/tests/snapstore_readiness.rs:107-130`] Optional: if the M4
      engine ends up depending on the exact signature of `get_snapshot`/`resolve_pages`,
      promote those existence pins (`let _ = SnapstoreClient::method;`) to typed signature
      pins like the existing `_put_pages_signature`. Leave as-is otherwise — the
      existence-only choice is deliberate and documented.
- [ ] [`crates/dh-snapshot/tests/snapstore_readiness.rs:159-163`] Optional defensive nit:
      reference a pin function from inside the `#[test]` body (e.g. `let _ = _surface_pins;`)
      so a future reader does not delete the "unused" pin functions. Not required.
- [ ] [`docs/ops/test-partitioning.md:172`] Optional readability: the aarch64 build/clippy
      table cell is now very long. Consider moving the verbose cross-compile recipe into a
      short subsection below the table with a one-line pointer in the cell. Content is
      accurate; readability only.
