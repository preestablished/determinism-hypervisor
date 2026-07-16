# Action Items

Verdict: **APPROVE.** Nothing below blocks the merge. All items are
forward-looking hardening or doc/consistency nits.

## Critical
- [ ] None.

## Important
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs:16-44] The bare `let _ =
      SnapstoreClient::method;` pins catch renames/removals/visibility changes but **not**
      signature drift, yet the module doc promises the gate "breaks here … instead of deep
      inside the M4 snapshot engine." Either (a) soften the doc to state that only existence
      is pinned for most methods and only `put_pages` has a real signature pin, or (b) add
      return-position-`impl-Future` signature pins (like `_put_pages_signature`) for the
      legs the M4 engine commits to first — at minimum `put_snapshot`, `get_snapshot`,
      `resolve_pages`. Note: `SnapshotRef`/`LogId`/`PageHash` are re-exported by
      `snapstore-types`, not `snapstore-client`; use the returned-future trick to avoid a
      second dev-dep, or add `snapstore-types` as a dev-dep if spelling the types is
      preferred. (See 01 §I1.)
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs:38-43] Blocking-facade pins omit
      `blocking::put_input_log` / `blocking::get_input_log`, though the async block pins both
      and the sibling exposes both (`blocking.rs:74-80`). Add the two missing pins for
      coverage parity. (See 01 §I2.)
- [ ] [Cargo.toml:44 / repo onboarding] Three sibling path deps (`../control-plane`,
      `../guest-sdk`, `../snapshot-store`) are now required for *any* cargo invocation; a
      lone clone fails with an opaque metadata-resolution error. Add a one-line pointer in
      README/CONTRIBUTING listing the three required sibling checkouts and their relative
      paths (optionally a preflight check). Doc follow-up, not a code change. (See 01 §I3.)

## Suggestions
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs:48-58] Also construct a
      `Transport::Auto { .., page_channel_path: None }` arm to pin the `None` shape the
      no-fast-path engine construction uses. (See 02 §S1.)
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs:10-12] Doc name-drops API.md §1.2
      `hashes_only` but nothing pins `resolve_pages`'s `hashes_only` arg; either add a
      `resolve_pages` signature pin or trim the doc reference. (See 02 §S2.)
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs (whole file)] When the M4 engine and
      `snapstore-server` test fixtures land, consider replacing/augmenting the fn-item wall
      with one in-process `put_pages -> put_snapshot -> get_snapshot` round-trip to pin the
      real wire contract. File a follow-up bead. (See 02 §S3.)
- [ ] [crates/dh-snapshot/tests/snapstore_readiness.rs:60-62] `_error_pins` proves
      `ClientError: Error` but not specific variants; add a `match` pinning the variant set
      the engine will handle (`MissingPages`, `BatchBlake3Mismatch`, `CasFailed`, …) when the
      engine commits to them. (See 02 §S4.)
- [ ] [docs/ops/test-partitioning.md:18] The aarch64 table cell now packs blake3-NEON +
      zstd-sys + no-sudo-clang guidance into one row; consider lifting the no-sudo path to a
      footnote/sub-section so the table stays scannable. Cosmetic. (See 02 §S5.)
- [ ] [.github/workflows/ci.yaml:54-57] Optionally reinforce, near the checkout step, the
      `repo/..`-relative coupling that makes `../snapshot-store/crates/snapstore-client`
      resolve. The top-of-file comment already lists all three siblings, so this is optional.
      (See 02 §S6.)
