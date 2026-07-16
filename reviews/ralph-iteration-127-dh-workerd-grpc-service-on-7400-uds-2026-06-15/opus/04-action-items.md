## Action Items

### Critical

- None.

### Important

- [ ] Route `VerifyReplay` KVM execution through worker resource ownership: allocate/account a verification slot or dedicated verifier actor, pin execution to the configured core before opening `InstRetired`, and ensure cleanup releases the slot/resource on every success and error path.

- [ ] Honor or explicitly reject `bisect_on_divergence`: if true bisection is still M8, make divergent requests with `bisect_on_divergence: true` return `Unimplemented` instead of emitting fabricated bisection fields; add a divergent service-level test for the selected behavior.

- [ ] Replace the temporary divergence proto mapping with a documented coarse contract or true M8 mapping: do not encode raw hash pairs as proto `reg_diff` without documentation/tests, and do not use an undocumented `u64::MAX` sentinel for `first_bad_epoch`.

- [ ] Enforce an application-level inline `VerifyReplay.input_log` byte limit before `LogReader::parse`, aligned with the proto/snapshot-store input-log contract, and add a test that oversized inline logs are rejected with a stable status code.

- [ ] Stop holding the shared snapshot-store client mutex across the entire KVM replay: refactor the replay path or client ownership so the mutex is held only during actual snapshot-store operations.

### Suggestions

- [ ] Add service-level negative tests for missing `VerifyReplay.log`, malformed `input_log_id`, invalid SILG container payloads, invalid DHILOG bytes, and divergent replay mapping.

- [ ] Consider streaming `EpochOk` progress while verification runs instead of collecting all progress events into a `Vec` and returning a stream only after replay completes.

- [ ] Reduce duplicate DHILOG parsing by moving log-writer reconstruction closer to the replay parser or by passing parsed header data through a helper.

- [ ] Add a follow-up bead for true M8 divergence bisection if this iteration intentionally ships only coarse divergence reports.
