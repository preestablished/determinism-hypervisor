# Positive Notes

## The decision reframes the trade-off instead of accepting it

The bead posed "hermetic vs fast." The doc's strongest move is refusing the
dichotomy: it identifies that the "build-and-spawn is slower" framing only
holds if you spawn a *second binary*, and the sibling's `serve_for_tests`
seam means there is no second binary and no rebuild — the store builds once
as a workspace dev-dep and starts in milliseconds. Once that's seen, the
provisioned service's *only* advantage evaporates and the rejection writes
itself. That's the right shape for a decision doc: it doesn't just pick, it
dissolves the apparent cost.

## Rejection of the provisioned service is argued on the axes that matter

The doc doesn't hand-wave "provisioning is annoying." It names the two
concrete failure modes a determinism suite cannot tolerate: HEAD-coupled
*drift* between a long-running server and the path-dep client, and *shared
mutable state* across runs ("a determinism suite's nightmare,"
lines 17-18). Both are real; both are eliminated by construction in the
in-process model (one sibling HEAD, one fresh `TempDir` per test). This is
exactly the analysis the bead asked for.

## The retry-semantics discovery is encoded where it bites, not buried

The non-obvious truth — that `(pages_new, pages_deduped)` is unreliable
because the client transparently retries idempotent uploads — is captured as
an inline comment *at the assertion site* (store_joint.rs:80-85), not just in
the doc. A future author who tries to tighten `new + deduped == 3` into
`new == 3 && deduped == 0` will read the reason before they break it. This is
the right place for a hard-won semantic.

## Each test is load-bearing

No filler. The roundtrip test proves byte-identity *and* "ref only after
durability" (R12's core acceptance). The re-put test proves both dedup
(`new == 0`) and content-addressed ref stability (`r1 == r2`) — two distinct
guarantees. The isolation test proves the `TempDir`-per-store promise the doc
makes, using the sharpest possible probe (a ref minted in store A is an error
in store B). Three tests, three orthogonal invariants.

## Test-construction care: dedup can't accidentally collapse fixtures

`page(fill)` (store_joint.rs:40-45) sets `p[0] = fill.wrapping_add(1)` so
pages with different fills are genuinely distinct content, with a comment
saying exactly why ("so dedup can't collapse them"). Without this, two
"different" pages could hash-collide into one and silently weaken the dedup
assertions. Small, correct, and explained.

## FULL-manifest completeness is handled correctly

`full_container` (store_joint.rs:58-66) builds refs over *contiguous* pages
`0..N` via `enumerate()`, satisfying the builder's FULL-manifest completeness
requirement, with a comment naming that constraint. `DeviceBlob` is
constructed with `raw_len: bytes.len()` (line 51), keeping the declared
length honest against the payload. The fixtures respect the store's real
invariants rather than poking at a happy path.

## The seam is the right surface and its lifetimes are explicit

`spawn_store() -> (ServerHandle, SnapstoreClient, TempDir)` returns all three
ownership tokens the caller must hold, and the doc comment spells out the
contract ("hermetic per call, UDS-only, TempDir owns its life"). Returning
the `TempDir` (rather than leaking it) makes store lifetime lexically
scoped to the test — drop the tuple, the store's directory is gone. This is
the correct seam for qmp and 6hg to build on (with the S1/S2 caveats about
the blocking client / socket path being documentation gaps, not design
flaws).

## Parity with the proven sibling pattern

The 20ms settle delay, UDS-only transport, and `serve_for_tests` usage all
mirror `snapstore-client/tests/page_channel_fallback.rs` exactly. Reusing the
sibling's battle-tested setup constants (rather than inventing new ones)
minimizes the surface for spawn flakiness and keeps the two suites from
drifting.

## Docs kept in sync

The `test-partitioning.md` row is accurate and self-contained — it states the
exact command, that the test spawns the server in-process on a TempDir over
UDS, and crucially that it needs *only* the `../snapshot-store` checkout (no
provisioning, no KVM). That last clause is the operationally important one
and it's correct: I confirmed the tests are host-runnable with no KVM by
running them on this box.
