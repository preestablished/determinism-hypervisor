Contract coverage:

- The happy path covers the intended call chain through the public blocking client:
  `put_pages`, `put_snapshot`, `get_snapshot`, `resolve_pages(..., false)`, and `resolve_pages(..., true)`.
- The test pins byte-identical manifest retrieval by comparing `get_snapshot` output to the original encoded container.
- Payload mode is strongly checked: `resolve_pages(..., false)` must return the expected index, hash, and page bytes for each manifest entry.
- Hashes-only mode is now checked at two layers:
  - public blocking client result: payload is `None`;
  - raw generated gRPC stream: payload bytes are empty on the wire.
- Same-snapshot baseline mode is checked through both the public blocking client and raw generated gRPC path; both must return no pages.
- `MissingPages` is covered as a real server response, not only a compile-time enum pin. The mixed present/missing case verifies the error is not merely "all manifest pages absent"; it must report exactly the missing subset.

Required findings:

- None remaining.

Recommended:

- If dh-snapshot will branch on the missing-parent arm, add a separate delta-manifest test that expects `ClientError::MissingPages { page_hashes: [], parent_ref: Some(_) }`.

Optional:

- If missing-page hash order is meant to be part of the contract, document that next to the assertion. If only completeness matters, compare as sets to avoid over-pinning response order.
