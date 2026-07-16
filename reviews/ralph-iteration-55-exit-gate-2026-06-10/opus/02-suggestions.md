# Suggestions (non-blocking)

These are precision/transparency polish items. None affect the verdict; all are
optional. They exist only to keep the table maximally honest for a fresh M4
implementer who treats the cited numbers as reproducible specifics.

## S1 — Row 1 cites only the timer-sub-gate hash as "the" state hash

The table says: "every fingerprint identical (icount 2,000,000; state hash
`7e09ac13…`; timer delivered at 1,234,567)". There are actually TWO distinct
fingerprints: the plain sub-gate (`482edfed…`, timer=None) and the timer sub-gate
(`7e09ac13…`, timer delivered 1,234,567). The table presents the timer hash as
representative without saying so. This is not false — within each sub-gate every
run is identical, which is the load-bearing claim — but a reader could mistake
`7e09ac13…` for "the Phase-1 hash" when the plain-landing hash differs.

Suggested: note both hashes, or label `7e09ac13…` explicitly as the timer
sub-gate fingerprint. Cost: one clause.

## S2 — Row 5 timing numbers are fresh-run-specific and drift

The table cites "timer … 95.7 s" and "regression … 5.5 s". My re-runs landed at
91–95 s (timer) and 3.77–3.92 s (regression). These are wall-clock numbers that
vary with machine load, so they are not falsifiable claims and should not be read
as guarantees. The regression delta (5.5 s claimed vs ~3.9 s observed) is large
enough to notice. Consider either dropping the seconds (they prove nothing about
determinism) or annotating them as "indicative, this run". The test verdicts
(green) are what matters and those reproduced cleanly.

## S3 — Row 5 "regression (1e9 ×2)" omits the 10M companion test

The `regression` suite has two tests: `ten_million_twice_equal_final_hash` and
`one_billion_instructions_twice_equal_final_hash`. The table cites only "1e9 ×2".
Both are green; mentioning only the larger one is fine as a headline, but a reader
auditing the suite name will find an extra (passing) test. Purely cosmetic.
