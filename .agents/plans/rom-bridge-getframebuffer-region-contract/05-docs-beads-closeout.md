# Docs, Beads, Handback, Closeout

## Beads (do this BEFORE writing code)

This repo mandates bd for all task tracking (no TodoWrite/markdown TODOs).

```sh
bd ready   # check nothing already tracks this request
bd search framebuffer   # ditto

MAIN=$(bd create "GetFramebuffer: derive geometry from D7 layout_version contract" \
  -d "Per .agents/requests/rom-bridge-getframebuffer-region-contract/ and plan .agents/plans/rom-bridge-getframebuffer-region-contract/. Replace 16-byte in-region descriptor parse and capture-path heuristic with layout-version-keyed geometry (v1 = XRGB8888 256x224 stride 1024, 229376 B). Rework nanokernel fixtures (resize capture_fixture FB region, delete framebuffer_fixture). Blocks rom-operator-bridge-9z2 (their tracker)." \
  -t bug -p 1 -l impl --silent)
bd update $MAIN --claim
```

Optional follow-up beads to file (NOT to implement now):

- Worker-side orphan-slot hardening from `04-related-slot-leak.md`: at
  minimum a WARN when `RestoreSnapshot`/`CreateVm` hits `NoFreeSlot` while
  all slots are paused at the same icount; possibly lease expiry or an admin
  destroy path. Priority 3; the primary fix is bridge-side
  (`rom-operator-bridge-72o`).
- Share framebuffer layout constants via `detguest-wire` (guest-sdk repo)
  instead of dh-worker-local constants. Priority 4, nice-to-have only.

## Decision Record

Per repo memory: no `docs/specs/` or `docs/adr/` — local architecture
decisions go in **`docs/decisions/`** (siblings: `proto-seam.md`,
`base-image-cache-contract.md`, ...). Add
`docs/decisions/framebuffer-region-geometry.md` covering:

- Context: D7 defines the framebuffer region as raw pixels with geometry
  keyed to `layout_version`; the worker's 16-byte descriptor expectation
  came from ralph iteration 131/133 review fixes with no decision record and
  no guest-side counterpart; the shipped workload and READY snapshot conform
  to D7.
- Decision: geometry is a worker-side table keyed by manifest
  `layout_version` (v1 = XRGB8888 256×224 stride 1024, 229,376 B); unknown
  versions and wrong lengths are `FailedPrecondition`; no in-region
  descriptor for any known version; heuristic classification removed from
  the capture path because it made `FbInfo` frame-content-dependent.
- Consequences: adding a framebuffer layout means adding a table entry (and
  only that); an in-region header would require a new layout_version plus a
  D7 spec change; RGB565 stays proto-only until a layout defines it.

Also check `docs/upstream-divergences.md`: this change *removes* a
divergence from the planning docs. If the descriptor behavior is listed
there, delete/amend that entry; if not, no change.

## Handback To The Bridge Team

The request's "Deployment And Handback" section asks for the `main` commit
SHA containing the fix, noted in the request directory. After merge to main:

1. Add `.agents/requests/rom-bridge-getframebuffer-region-contract/05-resolution.md`
   containing: the merged `main` SHA (`git rev-parse` it — never guess),
   a one-paragraph summary of what changed (layout-version table, capture
   path included, fixtures reworked), the exact new error-message shapes for
   unknown-version / wrong-length (they log worker errors verbatim), and a
   note that **they** rebuild and restart the deployed `dh-workerd`
   themselves (worker restart invalidates their in-memory leases — do NOT
   restart the deployed worker for them).
2. Do not touch the deployed worker, its pid file, or
   `/run/dh/grpc.sock`. Deployment timing is explicitly theirs.

## Session Close (mandatory, from CLAUDE.md)

```sh
bd close $MAIN -r "GetFramebuffer + CaptureSpec.framebuffer derive geometry from layout_version per D7; merged as <sha>"
git pull --rebase        # BEFORE any local merge, never after (process memory)
# commit(s): code+fixtures+tests as one logical unit; decision doc + plan/request
# resolution notes may be a second commit. Keep unrelated m9_handoff.rs
# working-tree changes OUT of these commits.
bd dolt push
git push
git status               # MUST show "up to date with origin"
```

Work is not done until `git push` succeeds. If this host cannot run the
KVM/64-core-gated m6 suite, state that plainly in the bead close reason and
in `05-resolution.md` — never report a gate as passed that did not run.
