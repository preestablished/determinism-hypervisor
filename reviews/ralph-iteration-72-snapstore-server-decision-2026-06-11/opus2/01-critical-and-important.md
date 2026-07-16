# Critical & Important Findings

## Critical

**None.** The decision is correct, the tests pass, the code compiles cleanly on
both arches, and the target-gating is verifiably effective. Nothing here can
corrupt data, break the build, or produce a false test result.

---

## Important

### I-1 — The `put_pages` "retry flips to all-deduped (0,3) on a fresh store" claim is unsubstantiated and contradicted by the client's own retry policy

**File:** `tests/determinism/tests/store_joint.rs:80-85`

```rust
// Tuple is (pages_new, pages_deduped). The SPLIT is not assertable:
// the client transparently retries transient errors and a retried
// upload reports everything deduped (content-idempotent). The SUM is
// the invariant.
let (new, deduped) = client.put_pages(wire).await.expect("put_pages");
assert_eq!(new + deduped, 3, "every page accounted for");
```

**What the commit claims:** that on a FRESH store, under parallel test load,
`put_pages` was observed returning `(0, 3)` — all-deduped — because the client's
transparent retry re-sent already-committed pages.

**What I found by experiment:** I could not reproduce it. A probe that ran a
fresh-store first-put **246 times** (50 sequential + 32 parallel, repeated; pages
content-salted per iteration so no cross-iteration collision is possible) on the
real `serve_for_tests` server reported `(new=3, deduped=0)` **every single time —
0/246 anomalies**, including under `worker_threads = 8` concurrency and
`--test-threads=8`.

**Why the claim is mechanically implausible on this seam (root-cause analysis):**

1. `with_retry` (`snapstore-client/src/retry.rs:37-49`) only retries
   `ClientError::Status` with code `Unavailable | DeadlineExceeded`, or
   `ClientError::Transport(_)`. A normal, successful in-process UDS round-trip
   produces **none** of these — there is no network, no deadline pressure, and
   the server returns `Ok`. So `with_retry` makes exactly one attempt and the
   split is deterministic.
2. For the (0,3) flip to occur, the *first* attempt would have to commit pages
   to the pagestore (`service.rs` ingests per-batch as the stream arrives,
   lines 139-165) and *then* fail with a retryable error before the client
   receives the response. On a healthy in-process server that post-commit /
   pre-response failure window does not occur.
3. The sibling's own retry test asserts the **opposite** shape:
   `put_pages_retries_on_unavailable` (`snapstore-client/src/tests/test_cases.rs:558-574`)
   injects `Unavailable` on the first call and asserts `new == 1, deduped == 0`
   — because `FlakyServer.check_inject` fires *before* the stream is read
   (`flaky_server.rs:155-160`), so the retry stores everything fresh. There is no
   existing test anywhere in the sibling repo that produces an all-deduped retry
   result.

**More likely root cause of the original (0,3) observation:** not a client retry
semantic, but a content/state-freshness artifact of how the *original* test was
run — e.g. pages that were not actually unique across runs, or a store dir that
was not actually fresh, so the second `put_pages` of identical content deduped.
The current committed `page(fill)` helper (`store_joint.rs:40-45`) only varies
`p[0]`; within a single test the three pages are distinct, but nothing guarantees
distinctness across whatever sequence produced the (0,3). I could not find a
mechanism by which a *genuinely fresh* store yields all-deduped.

**Impact.** The weakened assertion (`new + deduped == 3`) is itself **safe** — it
cannot produce a false failure and is a defensible invariant. The problem is the
**comment**, which is the load-bearing artifact: this file is explicitly "the
shared seam the snapshot engine (qmp) and M4 ACCEPT (6hg) build on" (lines 1-5),
and those downstream authors will read "the split is not assertable" as an
established client contract. As written, it documents a client behavior that the
client's retry policy does not exhibit on a healthy server.

**Recommended fix.** Either (a) re-capture the original (0,3) with a deterministic
repro and cite the exact failure mode, or (b) correct the comment to state the
true invariant and drop the unverified mechanism. The honest version:

```rust
// On a FRESH store, every page is new: (pages_new, pages_deduped) == (3, 0).
// We assert only the SUM here because the (new, deduped) split is a
// content-idempotency property of the store, not a stable per-call contract:
// the client *would* retry on a transient transport error (retry.rs), and a
// post-commit retry re-sends already-stored pages, shifting the split toward
// deduped while the sum is invariant. That retry path does not fire against a
// healthy in-process server (verified: 246/246 fresh first-puts returned
// (3,0)), so a stricter `assert_eq!(new, 3)` would also be correct here — the
// sum assertion is the deliberately conservative choice for the qmp/6hg seam.
let (new, deduped) = client.put_pages(wire).await.expect("put_pages");
assert_eq!(new + deduped, 3, "every page accounted for");
```

If the maintainers prefer to keep the test maximally strict (and catch a future
regression where a fresh first-put unexpectedly dedups), tighten it to
`assert_eq!((new, deduped), (3, 0))` — my experiments show that is stable. The
sum-only form is acceptable; the *false mechanism in the comment* is the defect
to fix.
