# Critical & Important Findings

**None.** No Critical and no Important findings.

The decision is sound, the implementation matches it, the retry-semantics
discovery is correct, and all three tests pass deterministically across
parallel and single-threaded runs. Everything below the bar lives in
`02-suggestions.md` as non-blocking documentation/robustness improvements.

## Verification notes (why nothing rose to Important)

### The retry-semantics discovery is correct, not a guess

The doc and `store_joint.rs:80-85` claim the `(pages_new, pages_deduped)`
split is unassertable because the client transparently retries transient
errors and a retried upload reports everything deduped. Verified against
the sibling source:

- `snapstore-client/src/client.rs:84-85`, `:128` — `put_pages` wraps the
  *entire* upload in `with_retry(...)`; the operation is content-idempotent
  (server deduplicates by hash).
- `snapstore-client/src/retry.rs:37-48` — `is_retryable` returns `true` for
  any `ClientError::Transport(_)` and for `Unavailable`/`DeadlineExceeded`
  status codes.

On a freshly-spawned UDS server the 20ms settle is marginal under load, so
a first attempt can land its pages and then hit a transient transport error
before the response is read; the retry re-sends the *same* content, the
server already has every page, and the second attempt reports `(0, N)`.
That is exactly the observed `(0,3)`-on-fresh. The fix — assert
`new + deduped == 3` — is the correct invariant.

### The parallel-interference alternative is ruled out

I ran the suite with `--test-threads=1` (RUN 3): still 3/3 green, same
`(0,3)`-tolerant assertion never triggering a different split. Because each
test mints its own `TempDir` store (`spawn_store` line 19), there is no
shared server and no cross-test page namespace — parallel interference
*cannot* produce the fresh-put split. Server-side batching is likewise not
a cause: batching changes how pages are framed (≤256/msg, client.rs:141),
not whether a page counts as new vs deduped. The retry explanation is the
right one.

### The re-put assertions are retry-proof — confirmed

`store_joint.rs:118-119` asserts `new == 0` and `deduped == 2` on the
second put of already-stored pages.

- `new == 0`: guaranteed under the *same* reasoning — once a page's content
  is stored, no later put of that content can ever be "new," regardless of
  how many times either put is retried. Content addressing makes "new" a
  permanent one-way transition. Confirmed.
- `deduped == 2`: the first put completed successfully (`expect("first
  put")`), so both pages are durably present before the second put begins.
  Whether the *second* put's internal retry fires or not, every page it
  sends is already stored, so the split is `(0, 2)` on the first attempt and
  stays `(0, 2)` on any retry. A retry cannot change a fully-deduped result
  into anything else. Confirmed retry-proof.
