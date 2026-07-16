# Review Overview

Branch: `ralph/iteration-153-persist-lapic-state-in-dhsnap-state-hash`
Date: 2026-06-18
Reviewer: Claude Opus (2nd reviewer)

This checkpoint replaces the empty DHSNAP LAPC placeholder with a typed LAPC v2 section, threads `LocalApic` through snapshot, restore, fork, service runtime, and replay paths, folds framed LAPC bytes into production replay state hashes, and rebaselines the DHSNAP/DHILOG fixtures with focused LAPC coverage. The normal snapshot, restore, fork, service-run, replay hash, and corpus paths are mostly cohesive, but the VerifyReplay bisection probe path still captures reset LAPC instead of the replay rail's live LAPC, making bisection evidence inconsistent with the new hash contract.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 27
- Lines added: 1294
- Lines removed: 219
- Commits: 1 (`dae068e`)
