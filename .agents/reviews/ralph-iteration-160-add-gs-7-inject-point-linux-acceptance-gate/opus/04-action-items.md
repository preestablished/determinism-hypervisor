# Action Items

- Add fixture evidence for workload-observed `FaultDecision` values and assert that sequence exactly matches the recorded `PORT_INJECT` `PIO_ANSWER` values before treating `determinism-hypervisor-bid` as accepted.

- Scope `InjectQuery` evidence to the post-READY GS-7 segment. Drain the READY backlog before the segment or filter by the segment boundary, and do not require absolute `iseq` to start at zero if earlier SDK calls may have advanced the counter.
