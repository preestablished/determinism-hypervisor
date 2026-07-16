# Critical & Important findings

## Critical

**None.** Every adversarial input to the comparator fails closed (drift,
exit 1), and the two failure modes that would silently brick the merge
gate were verified live and are correct.

---

## Important

### I1. The "pusher must be admin" premise — verified, but the prompt's identity was wrong (no action, document it)

The iteration prompt asserted the merge pusher is `infra-admin` and that
this user must be repo admin or the next merge push fails. Checked live:

- `gh api .../collaborators/infra-admin/permission` → **`read`**
- `gh api .../collaborators/mattsp1290/permission` → **`admin`**
- `ssh -T git@github.com` authenticates as **`mattsp1290`**
- Every recent `main` commit is authored AND committed by
  `Matt Spurlin <mattsp1290@gmail.com>`.

`infra-admin` is the local OS username, not the GitHub push identity. The
GitHub identity that actually pushes is `mattsp1290`, who **is** admin, so
with `enforce_admins=false` the Ralph flow can push reviewed work directly
past required checks. **The gate is correctly bypassable by the real
pusher — no action needed.** Flagging because the premise in the brief was
false; if the push credential ever changes to a token scoped to
`infra-admin` (read-only), direct pushes to `main` would fail entirely.
Recommend: confirm the CI/automation push token maps to `mattsp1290` (or
another admin) and not to `infra-admin`.

### I2. `set -e` + pipefail interaction is SAFE here, but only by an assignment exemption — worth a comment

The brief flagged a real hazard: under `set -euo pipefail`, the comparator
calls `got="$(live_value "$key")"`, and `live_value` for the known keys
pipes `grep -m1 ... | cut | sed`. If `grep` ever finds nothing (a future
kernel renames `model name`, or `microcode` is absent in a VM/container),
the pipeline exits nonzero.

Tested directly: bash **does not** trigger `set -e` for a failed command
substitution inside a *simple assignment*. So `got` becomes the empty
string and the key is reported as drift (exit 1) — fail-closed, clean, no
mid-loop crash. The bogus-key case (`*) echo "<unknown key>"`) is doubly
safe because `echo` is a builtin that never fails under pipefail.

This is correct behavior, but it survives by a subtle bash rule, not by
design intent in the code. **Recommend a one-line comment** at the
`got="$(live_value ...)"` site noting that a missing live field
intentionally yields empty-string → drift (fail-closed), so a future
refactor (e.g. moving the call out of an assignment, or `local got;
got=...` which has the *same* exemption but is easy to "fix" wrongly)
doesn't accidentally convert a missing-field into an unclean abort.
This is the only place engine-adjacent fragility could creep in.

### I3. No `.gitattributes` pinning the lock to LF — CRLF would falsely red the nightly

Verified: a CRLF copy of the lock makes **all 7 keys** falsely report
drift (the `\r` is appended to every value; byte-exact compare fails while
the values look identical in the log — the classic CRLF tell). The repo
has **no `.gitattributes`**, and `git check-attr` shows the lock as
`text: unspecified`. Risk is low (this is a lab box, edits happen on
Linux), but a single Windows/editor round-trip on the lock would turn the
nightly red across the board with a baffling "GenuineIntel != GenuineIntel"
report. **Recommend** adding `ci/determinism-class.lock text eol=lf` (or
`* text=auto eol=lf` repo-wide) to a `.gitattributes`. Cheap insurance for
a file whose entire job is byte-exact comparison.
