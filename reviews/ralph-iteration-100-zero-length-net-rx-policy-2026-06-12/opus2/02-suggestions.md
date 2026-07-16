# Suggestions (non-blocking)

## S1 — Pin `MAX_FRAME == MAX_NET_RX_FRAME` at compile time

**File:** `crates/dh-devices/src/net.rs:49-51`

`MAX_FRAME` is a bare `pub const MAX_FRAME: u32 = 2048;` whose doc-comment claims it "mirrors
`dh_inputlog::dhilog::MAX_NET_RX_FRAME`". That mirror is enforced only by prose. `dh-devices`
**already depends on `dh-inputlog`** (`Cargo.toml:10`, and `net.rs:29` imports
`dh_inputlog::dhilog::LogWriter`), so a const-assert is cheap and turns a comment into a guarantee:

```rust
pub const MAX_FRAME: u32 = dh_inputlog::dhilog::MAX_NET_RX_FRAME as u32;
```

or, if keeping the literal is preferred for readability:

```rust
const _: () = assert!(MAX_FRAME as usize == dh_inputlog::dhilog::MAX_NET_RX_FRAME);
```

This matters more now than before: the codec just tightened to `1..=2048` and a future cap bump
on one side that silently desyncs from the other would re-open the kind of cross-layer mismatch
bead 206 just closed (e.g. the device accepting a frame the codec rejects, or vice versa). The
existing fault tests would not catch a cap *increase* on only one side. `MAX_NET_RX_FRAME` is
already `pub`, so no visibility change is needed.

## S2 — Note the validation tightening on the `lyu` inspection bead

**Bead:** `lyu` — "DHILOG: inspection-only entry point for unsealed crash artifacts"

The reader is the planned foundation for the future crash-artifact inspection path (`lyu`:
"framing/totality validation… returning records best-effort"). Tightening `validate_kind` so that
a zero-length NET_RX is now a hard `BadPayloadLayout` narrows what a future inspector can surface
about a *historical or hostile* artifact: an operator debugging a corrupt log that happens to
contain a len-0 NET_RX field would, under the current reader, get a wholesale parse rejection with
no record-level detail.

That is the correct behaviour for the *replay* `parse` path (this change). But when `lyu` builds
its best-effort `parse_unsealed`, it should decide deliberately whether codec-validity rules like
`1..=2048` are *replay invariants* (skip them in inspection mode, report the record with a flag) or
*structural invariants* (still enforce). No action is needed in this branch — just worth a one-line
note on `lyu` so the inspection entry point doesn't silently inherit the replay-strictness and lose
diagnostic reach over old artifacts. (Flagging here rather than editing the bead; the author may
prefer to record it.)

## S3 — Optional: writer-side fuzz/seed coverage of the new lower bound

The fuzz target (`dhilog_parse`) only drives `LogReader::parse`; the tighter lower bound merely
*shrinks* the accepted-input space, so no corpus seed can break and none is required. If desired,
a single hand-added corpus seed containing a well-framed sealed log whose only NET_RX is exactly
1 byte would assert the new boundary stays reachable through the fuzzed accessors — but the unit
test `net_rx_frame_boundaries` already covers the 1-byte case, so this is genuinely optional.
