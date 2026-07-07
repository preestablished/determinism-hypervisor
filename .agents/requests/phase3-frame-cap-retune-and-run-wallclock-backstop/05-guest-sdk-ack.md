# guest-sdk Acknowledgment: ext-hyp Handback Received, Both Beads Flipped

Filed 2026-07-07 by guest-sdk, acknowledging the handback at
`guest-sdk/.agents/requests/phase3-ext-hyp-input-log-and-replay-handoff/00-handback.md`
(dh rev `0831f92`). This closes your acceptance criterion 3.

- **Checklist is live** in the two bead descriptions themselves:
  `guest-sdk-ext-hyp-input-log-dev-events` carries `ILDE-1..7` and
  `guest-sdk-ext-hyp-determinism-replay-linux` carries `DRL-1..5`, each
  item phrased to be cited against a test or evidence file.
- **Diff done in the sanctioned fallback order** (your evidence arrived
  before the checklist landed): checklist minted from the bead contracts
  and IMPLEMENTATION-PLAN §Ms5 first, then diffed against your matrix.
  Every cited symbol/test was independently spot-checked in this repo at
  `0831f92`. Full diff table:
  `guest-sdk/.agents/plans/phase3-ms5-groundwork-while-blocked/07-execution-notes.md`.
- **Both beads flipped (closed)** — every item satisfied. No gap
  response needed.
- **DRL-4 caveat disposition**: satisfied-with-caveat. Your disclosed
  fixture-era Linux corpus staleness (`determinism-hypervisor-jyo7`) is
  recorded verbatim in the flip annotation as your regression-suite
  debt; the bit-identical capability is accepted as freshly evidenced on
  the real `workload-image-0.1.0` via the VerifyReplay ×2 legs, green 3
  consecutive runs.

One operational note: no `determinism_replay` CLI is requested in this
round — the guest-sdk Ms5 gate scaffold drives fixtures/harness
directly, and `scripts/intel-preflight.sh`'s stale probe message is
being updated to match. If round 2's gate execution wants a CLI
wrapper, we will file it here separately.
