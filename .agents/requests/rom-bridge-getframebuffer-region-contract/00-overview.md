# Request: GetFramebuffer Must Honor The D7 Raw-Pixel Region Contract

## Who Is Asking

The `rom-operator-bridge` project (operator UI + service that drives real
sessions against `dh-workerd` over the worker gRPC API). Filed 2026-07-01.

Tracking on the bridge side:

- `rom-operator-bridge-9z2` — frame endpoint returns 503 while a real session
  is paused (diagnosis complete; blocked on this request)
- `rom-operator-bridge-72o` — related but separate slot-leak issue, see
  `04-related-slot-leak.md`

## What Is Blocked

The bridge's live frame preview (`GET /api/frame/current`, backed by
`HypervisorWorker/GetFramebuffer`) fails on **every** call against a real
session restored from the rom-bridge-o73 READY snapshot. The operator UI shows
"Backend unavailable." instead of the guest framebuffer, which blocks
browser-based visual validation of guest state — the reason the bridge exists.
This is currently our top blocker.

Deployed worker at time of diagnosis: debug `dh-workerd` on this host, binary
built 2026-06-25 (repo tip was `cf1a383` "Harden M9 READY handoff outputs";
tree cleanliness at build time not recorded). The bridge logs every worker RPC
failure with gRPC code + message (`journalctl -u rom-operator-bridge`, WARN),
which is where the evidence in `01-observed-behavior.md` comes from.

## The Ask In One Paragraph

`dh-worker`'s `GetFramebuffer` currently requires the guest's published
framebuffer region to begin with a 16-byte `width/height/stride/format`
descriptor header. The reference-workload region contract (ARCHITECTURE D7)
defines that region as **raw pixels only** — XRGB8888, 256×224, row-major,
stride 1024, 229,376 bytes, `layout_version 1` — with no in-region header, and
the detguest-wire manifest entry carries no geometry fields. The guest matches
its spec; the worker parses pixel bytes as a descriptor and rejects every
request. We need `GetFramebuffer` — and the `CaptureSpec.framebuffer` path
used by `Run`/`TakeSnapshot`, which guesses at a descriptor heuristically and
can silently emit wrong geometry (see `02-root-cause.md`) — to derive
geometry from the region contract instead of an in-region header.

## Files In This Request

| File | Contents |
|---|---|
| `01-observed-behavior.md` | Empirical evidence: exact errors, how to reproduce |
| `02-root-cause.md` | The contract mismatch, with file/line references on both sides |
| `03-requested-change.md` | What we need, suggested approach, acceptance criteria |
| `04-related-slot-leak.md` | Context on the NoFreeSlot exhaustion we hit on the way |
