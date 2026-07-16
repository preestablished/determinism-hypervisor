# Positive Notes

- **Honest "AS BUILT" documentation.** The doc edit replaces an aspirational
  example (`group: kvm-intel-${{ github.workflow }}`, cancel false for *both*)
  with the real, deliberately-split configuration and explains *why* each side
  differs (stateless CI → cancel true; measurement nightly → cancel false). This
  is exactly the right instinct: docs that record reality with rationale, not
  idealized snippets that silently diverge from the workflows.

- **Correct cancel-in-progress polarity.** Stateless CI uses `true` (collapse
  stale superseded runs, keep the single box's queue clean); the measurement
  workflow uses `false` (never kill a determinism/canary run mid-flight). The
  determinism product's core value — trustworthy measurements — is protected by
  the conservative choice exactly where it matters.

- **Tight, well-commented diff.** Both the YAML comment ("never cancel in
  flight — this is the measurement workflow") and the doc cross-reference each
  other, so a future reader hitting either file is pointed at the other.

- **Bead closed honestly with a live audit trail.** The 6eb notes record the
  one discrepancy found during verification (nightly lacked the prescribed
  concurrency group) and how it was fixed, plus byte-for-byte confirmation of
  the security policy. That is the discrepancy this very diff resolves — a clean,
  self-documenting loop.

- **End-to-end verification actually exercised KVM.** The full workspace suite
  passed with the live-KVM test legs running for tens of seconds each (not
  self-skipping), independently confirming the runner user's /dev/kvm rw access
  that the bead claims.
