Branch name: ralph/iteration-109-decide-forkrequest-entropy-seeds-versus-fork-engine-prng-contract
Date: 2026-06-15
Reviewer name: Claude Opus (2nd reviewer)

The change chooses a compatible ForkRequest entropy contract: absent or all-zero child seeds continue the fork-point PRNG, while non-zero child seeds start fresh deterministic child segment streams. It updates proto/API/architecture docs, adds mapper normalization, and extends fork_engine with optional reseeding.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 10 tracked files
- Lines changed: 215 insertions, 11 deletions
- Commits reviewed: 0 branch commits; working-tree diff reviewed

