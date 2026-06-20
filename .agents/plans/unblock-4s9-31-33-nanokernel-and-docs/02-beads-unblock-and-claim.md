# Beads Unblock And Claim

## Preflight

Run from repo root:

```bash
bd prime
git status --short --branch
bd show determinism-hypervisor-4s9.31
bd show determinism-hypervisor-4s9.33
```

Confirm all dependencies shown by each bead are closed. If any listed dependency has reopened, stop and reassess.

## Unblock And Claim 4s9.31

Preferred sequence:

```bash
bd update determinism-hypervisor-4s9.31 \
  --status open \
  --append-notes "Unblocking: listed dependencies 4s9.24, 4s9.26, 4s9.28, 4s9.29, and 4s9.7 are closed; starting post-M9 nanokernel preservation evidence."
bd update determinism-hypervisor-4s9.31 --claim
```

If team convention says not to move a stale-blocked bead to `open` manually, use a Beads comment instead and then try `--claim`:

```bash
bd comment determinism-hypervisor-4s9.31 "Starting post-M9 nanokernel preservation evidence now that all listed dependencies are closed."
bd update determinism-hypervisor-4s9.31 --claim
```

If `bd update --claim` refuses while status is `BLOCKED`, use the explicit `--status open` path above. `bd update --help` confirms `--status` and `--claim` are supported.

## Unblock And Claim 4s9.33

After `4s9.31` is closed, or in a separate serial session if the operator wants docs work first:

```bash
bd update determinism-hypervisor-4s9.33 \
  --status open \
  --append-notes "Unblocking: listed dependencies 4s9.22, 4s9.24, and 4s9.29 are closed; starting Linux gate command, runner requirement, and CI/nightly classification docs."
bd update determinism-hypervisor-4s9.33 --claim
```

Same fallback applies if manual status changes are not desired:

```bash
bd comment determinism-hypervisor-4s9.33 "Starting Linux gate command and runner classification docs now that all listed dependencies are closed."
bd update determinism-hypervisor-4s9.33 --claim
```

## Ordering

Recommended order:

1. Complete `4s9.31` first, because `4s9.32` depends on it and the evidence will feed the phase gate docs.
2. Complete `4s9.33` second, because `4s9.34` and `4s9.35` depend on it.

Do not bundle `4s9.32` or `4s9.34` edits into this work unless the user explicitly redirects. This plan is for unblocking and completing `4s9.31` and `4s9.33`.
