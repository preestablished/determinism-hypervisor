# Suggestions

- Pair each query occurrence with the corresponding answer occurrence and assert monotonic ordering, for example `answer.icount >= query.icount` and increasing answer sequence numbers. The replay path is occurrence-order based, so this would make the test's intended correlation explicit.

- Parse `NameIntern` events into a `name_id -> name` map and include names in failure output. The current `first_query_name_id` diagnostic is useful but still leaves the operator guessing which fixture call sites were exercised.

- Once the workload-observed decision echo exists, stream only the event kinds needed for the assertion, such as `InjectQuery`, `NameIntern`, and the chosen observation event. That keeps the gate less sensitive to unrelated fixture chatter.

- Consider making the "two distinct non-Proceed decisions" requirement explicit in the bead/docs if distinctness is mandatory. The current code and docs require distinct values, which is stronger than merely proving multiple nontrivial decisions.

- Include the decoded first few decisions in the `eprintln!` summary, not only counts. When this ignored gate fails on the lab box, the fastest diagnosis will be seeing the concrete packed and decoded values.
