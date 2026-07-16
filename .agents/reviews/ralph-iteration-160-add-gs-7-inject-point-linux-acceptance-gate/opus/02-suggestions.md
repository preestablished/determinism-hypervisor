# Suggestions

- Use `streams: vec![EventKind::InjectQuery as u32]` when collecting the GS-7 evidence if the test only needs inject queries. That avoids consuming unrelated SDK events and makes the evidence collection more focused.

- When the fixture publishes observed decisions, print both the packed value and `FaultDecision::unpack(value)` in failure/evidence output. Packed integers are canonical for comparison, but decoded values will make lab failures easier to triage.

- Consider pairing the observed decision sequence with `iseq` in the fixture evidence. The DHILOG `PIO_ANSWER` record does not include `iseq`, so exact call order is currently the only available link between queries and answers.

- The new `common::input_log_payload()` helper is useful and mirrors code that existed locally in `linux_worker_api.rs`. If more tests need it, this shared helper is the right direction.
