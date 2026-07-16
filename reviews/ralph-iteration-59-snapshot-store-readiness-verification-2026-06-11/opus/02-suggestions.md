# Suggestions (non-blocking)

### SUG-1 — Surface pins use `let _ =` rather than typed bindings

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:107-130`
- **What/why:** The connection/take/restore/input-log/blocking pins are all
  `let _ = SnapstoreClient::method;`. This compiles only if the method exists and is
  callable as a function item, which catches *removal* and *privatization* — good. It does
  **not** catch a *signature change* (e.g. `put_snapshot` changing its argument type),
  because the function item is never coerced to a typed `fn`. Only `_put_pages_signature`
  pins a signature. This is a deliberate, documented trade-off ("Signatures are pinned
  where cheap"), so it is fine as-is — but if the M4 engine grows to depend on the exact
  shape of, say, `get_snapshot`/`resolve_pages`, consider promoting those to typed
  signature pins too:

  ```rust
  // Pins the signature, not just existence:
  let _: fn(&SnapstoreClient, SnapshotRef) -> _ = SnapstoreClient::get_snapshot;
  ```

  (Would require naming `SnapshotRef` in this crate's namespace, which is the reason the
  author avoided it — hence "where cheap." Leave unless a real consumer needs it.)

### SUG-2 — Empty `#[test]` body relies on the doc comment to explain itself

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:159-163`
- **What/why:** `snapstore_client_surface_is_present` has an empty body with a comment
  explaining the real work is the compile-time pins. This is correct and idiomatic for a
  compile-gate test. Minor optional improvement: reference one pin function so a future
  reader who deletes the "dead" `_surface_pins` fn (thinking it is unused) gets a nudge —
  e.g. call `let _ = _surface_pins;` inside the test, or add `#![allow(dead_code)]` intent
  via a module comment. Not required: the functions are already exercised at compile time
  and Rust will not warn on them in a test crate the way it would in a lib. Purely
  defensive.

### SUG-3 — Consider a brief note that the dep is intentionally consumer-less at runtime

- **File:** `Cargo.toml:39-44` (workspace dep comment)
- **What/why:** The comment already explains "the only consumer is dh-snapshot's
  snapstore_readiness dev-dep surface-pin test." This is good and pre-empts the "dead
  workspace.dependencies entry" pitfall flagged in the cargo research. No change needed;
  noted as a positive. (If a future cleanup pass ever questions why a heavy gRPC dep has no
  runtime consumer, this comment is the answer — keep it.)

### SUG-4 — `test-partitioning.md` aarch64 row is now a very long single cell

- **File:** `docs/ops/test-partitioning.md:172` (the updated `aarch64 build/clippy` row)
- **What/why:** The row correctly adds the `zstd-sys via snapstore-client` note to the
  cross-build C-toolchain guidance, and the no-sudo clang fallback is genuinely useful. The
  cell is now quite long for a Markdown table. Optional: move the verbose cross-compile
  recipe into a short subsection below the table and leave a one-line pointer in the cell.
  Non-blocking readability nit only; content is accurate.
