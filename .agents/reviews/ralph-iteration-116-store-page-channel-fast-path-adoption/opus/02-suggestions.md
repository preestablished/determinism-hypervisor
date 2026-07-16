# Suggestions

Non-blocking:

- Consider making the normal `spawn_store_at` helper's page-channel connection observable in a future sibling/client API. The current proof is indirect: `spawn_store_at_with_corrupt_page_channel` proves the corrupt fixture uses the channel, and the normal helper follows the same construction path, but `SnapstoreClient::connect` does not expose whether `try_connect_page_channel` actually attached. If the socket file exists before the listener is fully connectable, `Transport::Auto` can still build a working gRPC client and the normal fixture would not know it fell back.
- The readiness loop in `common/mod.rs` waits for the page-channel socket path to exist before constructing the Auto client. That is a reasonable test fixture gate, but path existence is weaker than a successful page-channel handshake. The corrupt-cross-check test catches this for its own helper path; an explicit "must use page channel" signal would make the broader worker fixture adoption less inferential.
- The updated `snapstore_readiness.rs` comments correctly describe the new live-path owner. If the sibling `Transport` docs still call `page_channel_path` reserved/unused, update those in the sibling repo when that code is part of the same acceptance trail.
