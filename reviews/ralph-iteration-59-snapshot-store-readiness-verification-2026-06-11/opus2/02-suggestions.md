# Suggestions (non-blocking)

### S1 — Pin `Transport::Auto.page_channel_path` as `Some(..)` *and* `None` to lock the option shape

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:48-58`
- **What/why:** `_transport_pins` constructs `Transport::Auto { .., page_channel_path:
  Some(uds.clone()) }`. This pins the field's existence and that it accepts
  `Option<PathBuf>`. It already does the job. A marginal hardening: the M4 engine on a
  non-localpath box will construct `page_channel_path: None`, so pinning the `None` arm too
  documents that the engine's own "no fast path" construction stays valid. Minor — the
  `Some` arm already proves the type is `Option<PathBuf>`.

  ```rust
  Transport::Auto { uds_path: uds.clone(), tcp_addr: tcp.clone(), page_channel_path: None },
  ```

### S2 — Module doc references "API.md §1.2 `hashes_only`" but the test never pins `resolve_pages`'s `hashes_only` arg

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:10-12`
- **What/why:** The doc says the page channel "is selected via `Transport::Auto`'s
  `page_channel_path` field, which the Transport pin below covers." Accurate. But it also
  name-drops `hashes_only` (the `resolve_pages` bool that controls whether page bytes come
  back) without pinning that `resolve_pages` takes it. The async `resolve_pages` signature
  is `(SnapshotRef, Option<SnapshotRef>, bool) -> ...`. If you add the I1 signature pins,
  include `resolve_pages` so the `hashes_only` mention in the doc is actually backed by a
  pin. Otherwise consider trimming the `hashes_only` reference from the doc to avoid
  implying it's pinned.

### S3 — Surface-pin file could grow into a many-method drift trap; consider a single round-trip smoke test instead, later

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs` (whole file)
- **What/why:** As the M4 engine commits to specific methods, this file will accrete more
  bare pins, each catching only renames. Research note: "Each integration test file adds a
  link step; group related assertions into one file" — this is already one file, good. But
  long-term, once `snapstore-server` test fixtures are available (the sibling already
  dev-deps `snapstore-server` for its own tests), a single in-process `put_pages ->
  put_snapshot -> get_snapshot` round-trip against an embedded server would pin the *real
  contract* (wire behavior, error variants) far more strongly than a wall of fn-item
  references — at the cost of pulling `snapstore-server` into this repo's dev graph and a
  tokio runtime in the test. Not for this PR; worth a follow-up bead alongside the M4 engine
  work.

### S4 — `_error_pins` pins `ClientError: std::error::Error` but not any specific variant

- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:60-62`
- **What/why:** `_error_pins` proves `ClientError` implements `std::error::Error` — a good,
  cheap pin. The engine's actual error handling will branch on specific variants
  (`MissingPages`, `BatchBlake3Mismatch` is the P0 determinism signal, `CasFailed`, etc.,
  per `error.rs:5-54`). A `match` that names the variants the engine must handle would pin
  the variant set the same way the method pins work:

  ```rust
  fn _error_variant_pins(e: &ClientError) {
      use snapstore_client::ClientError::*;
      match e {
          MissingPages { .. } | BatchBlake3Mismatch { .. } | CasFailed { .. } => {}
          _ => {}
      }
  }
  ```

  Non-blocking, and arguably premature until the engine commits to handling them — note
  here, decide when the engine lands.

### S5 — Docs: the aarch64 cross-compile note is now quite dense; consider a sub-bullet

- **File:** `docs/ops/test-partitioning.md:18`
- **What/why:** The aarch64 row now packs blake3-NEON *and* zstd-sys cross-compile guidance
  plus a no-sudo clang fallback into one table cell. It's accurate and useful (I verified
  zstd comes in via `snapstore-manifest`, a transitive dep of snapstore-client). The cell is
  getting long for a markdown table. Optionally lift the no-sudo path into a footnote or a
  short sub-section below the table so the table stays scannable. Purely cosmetic.

### S6 — Consider asserting the sibling checkout path depth in a CI comment or guard

- **File:** `.github/workflows/ci.yaml:54-57`
- **What/why:** The path dep is `../snapshot-store/crates/snapstore-client`, resolved
  relative to *this* repo's checkout at `path: repo`. The sibling is checked out to `path:
  snapshot-store` (a sibling of `repo`), so `repo/../snapshot-store` resolves correctly. This
  is the same layout the other two siblings rely on and it works. A one-line comment near
  the checkout, or reusing the existing top-of-file comment's path list, makes the
  `repo/..`-relative coupling explicit for the next person who edits the workflow. The
  top-of-file comment (ci.yaml:18-21) already lists all three — good enough; this is
  optional reinforcement.
