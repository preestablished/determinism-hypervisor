# Plan: GetFramebuffer Honors The D7 Raw-Pixel Region Contract

## Source Request

`.agents/requests/rom-bridge-getframebuffer-region-contract/` (filed 2026-07-01
by the rom-operator-bridge project, committed here as `a38a1a0`). Read all four
request files before starting — they are accurate; every code claim in them was
re-verified against the current tree while writing this plan.

## Goal In One Paragraph

`dh-worker` currently parses the first 16 bytes of the guest's published
framebuffer region as a `width/height/stride/format` descriptor. The
reference-workload D7 contract defines that region as raw pixels only
(XRGB8888, 256×224, stride 1024, 229,376 bytes, `layout_version 1`), so
`GetFramebuffer` fails on every call against a real session and the
`CaptureSpec.framebuffer` path emits frame-content-dependent (non-reproducible)
geometry. Fix: derive framebuffer geometry from a layout-version-keyed table
(the manifest entry's `layout_version`, which the worker already resolves and
then discards), delete the in-region descriptor parse and the capture-path
heuristic, and reject unknown layout versions / wrong-length regions with a
`FailedPrecondition` naming the offender. This adopts the request's "Suggested
Approach", which we confirm is the right one (see `02-contract-and-decision.md`).

## Scope

- **In scope (this repo only):** `crates/dh-worker/src/service.rs`,
  nanokernel test fixtures (`tests/nanokernel/`), dh-worker unit +
  integration tests, a decision record in `docs/decisions/`, a handback note
  in the request directory, beads bookkeeping.
- **Out of scope:** guest-sdk (`detguest-wire`) changes — the request
  explicitly allows defining the geometry constants locally in `dh-worker`
  with a D7 citation. No guest, snapshot, or bridge changes. No proto changes
  (`PixelFormat::Rgb565` stays in the proto; no layout maps to it). The slot
  leak in `04-related-slot-leak.md` is bridge-side (`rom-operator-bridge-72o`);
  do NOT implement worker-side lease expiry in this change — at most file a
  bead (see `05-docs-beads-closeout.md`).

## Critical Finding Not In The Request

The in-repo nanokernel test fixtures **collide with the new contract**: both
`capture_fixture` (64 KiB raw region) and `framebuffer_fixture` (144-byte
descriptor-bearing region) publish a `framebuffer` region with
`layout_version 1`. Under the new contract, `layout_version 1` means "raw
229,376 bytes" and anything else is rejected — so these fixtures and every
test asserting their current behavior must change in the same commit. This is
most of the actual work. See `01-current-state.md` (inventory) and
`04-tests-and-fixtures.md` (strategy).

## Plan Files

| File | Contents |
|---|---|
| `01-current-state.md` | Verified code map with line refs; test/fixture inventory |
| `02-contract-and-decision.md` | The exact new contract, error messages, decision rationale |
| `03-implementation-sequence.md` | Ordered code changes in `service.rs` |
| `04-tests-and-fixtures.md` | Nanokernel fixture rework, test updates, new regression tests |
| `05-docs-beads-closeout.md` | Decision record, handback reply, beads, session-close protocol |

## Definition Of Done

1. All request acceptance criteria 1–3 pass as dh-worker regression tests
   (criterion 4 is verified by the bridge team post-deploy; we only hand back
   a `main` commit SHA).
2. Full workspace test suite green. This change alters capture output bytes
   (`fb_lz4`/`FbInfo`) which feed snapshot/replay paths — per repo process
   memory, run **3+ consecutive full workspace test runs** before merging
   anything determinism/hash-sensitive, and never chain `test ; commit`
   with a semicolon (gate on the exit code).
3. Decision record committed; handback note with the merged `main` SHA added
   to the request directory; beads closed; `git push` + `bd dolt push` done.
