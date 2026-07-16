Branch name: ralph/iteration-109-decide-forkrequest-entropy-seeds-versus-fork-engine-prng-contract
Date: 2026-06-15
Reviewer name: Claude Opus

The change resolves the ForkRequest entropy contract by documenting that omitted/all-zero seeds continue the fork-point PRNG and non-zero per-child seeds start fresh deterministic child segment streams, adding mapper validation, engine support, and focused proto/fork tests.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 10 tracked files
- Lines changed: 215 insertions, 11 deletions
- Commits reviewed: 0 branch commits; working-tree diff reviewed

