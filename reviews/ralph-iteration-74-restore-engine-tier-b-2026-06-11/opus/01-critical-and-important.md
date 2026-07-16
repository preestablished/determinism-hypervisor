# Critical and Important Issues

## Critical

**None.** No security, data-loss, crash, or broken-functionality issues were found.
The decode path is total (it delegates to `Container::parse`, which is already a
total decoder over untrusted bytes, plus typed `*::decode` calls that all
length/version-check), every `unwrap()` in the engine sits behind a dominating
length check or is on infallible conversions, and the documented "partial failure
⇒ scrap slot" contract is honest (see Positive Notes).

## Important

**None blocking.** The items below are observations I considered for Important
severity and concluded are *correct as written* given the decided design. I record
them so a future reader does not re-flag them.

### Verified-correct: restore ordering (RAM → devices → vCPU)

The §8.3 order is honored exactly. RAM is fully materialized and coverage-checked
(`restore_engine.rs:132-165`) before the first `DetDevice::restore` runs
(`:212-240`), so the EVTC re-attach precondition ("guest RAM is already restored",
`detchannel.rs:206-218`) holds — `Channel::attach` reads the live channel header.
The vCPU is restored last (`:270-275`), and `vcpu_state::restore` owns the
internal KVM_SET_* order (`vcpu_state.rs:143-191`). No device observes pre-restore
RAM.

### Verified-correct: `vns_base` set point

`PvClock::set_vns_base(time.vns)` runs at `:257-268`, *after* the device's own
`restore()` (which sets `timer_deadline_vns`/`timer_vector`) and *before* the vCPU
restore. Because `vns_base` is independent of those device fields and is not
captured in CLKD by design (`clock.rs:44-53`), the relative order of "device
restore" vs "set_vns_base" does not matter; placing it before the vCPU step is
fine. The transparency test (`full_restore_..._reseeds_the_segment_clocks`,
tests `:235-242`) pins the guest reading `vns == TIME.vns` at segment-relative
icount 0.

### Verified-correct: counter `IOC_RESET` placement

`counter.reset()` (`:277-281`) runs after the vCPU restore and before returning,
with no guest entry in between. `reset()` is a `PERF_EVENT_IOC_RESET` that zeroes
the accumulated count even while the event stays enabled (`counter.rs:121-128`),
so the next segment counts from 0. The delta test asserts the pre-restore count is
`> 0` and the post-restore read is exactly `0` (tests `:431-436`). Correct, and it
exercises the real perf path when available.

### Verified-correct: RAM coverage invariant vs the store wire format

The engine requires every page to arrive with a non-empty payload
(`:144-152`) and rejects any uncovered page (`:158-165`). I traced the store: a
no-baseline (`baseline_ref=None`), `hashes_only=false` resolve flattens the chain
down to the FULL root, which covers every page, and payloads are loaded — the
server only sends an empty payload (`unwrap_or_default()`,
`service.rs:348`, mapped to `None` in `client.rs:279`) for the `hashes_only`
path. The engine calls `resolve_pages(snapshot_ref, None, false)` (`:134-136`),
so a `None` payload genuinely indicates a broken store and the loud error is
correct, not a false positive. There is no zero-page elision on this path.

### Verified-correct: bidirectional shape strictness

Forward: each non-entropy bus device must find its section, else
`Codec("... has no section ...")` (`:227-231`); the entropy device must be present
(`:241-245`). Reverse: `total_sections != 5 + device_sections_consumed` is a loud
`Codec` error (`:246-255`), catching extra/foreign sections (the NETL test,
tests `:689-700`). The "5 fixed" count (MCFG, VCPU, LAPC, TIME, ENTR) is right —
ENTR absorbs the entropy device, which is deliberately *not* added to
`device_sections_consumed`. This exactly matches the capture-side layout in
`snapshot_engine.rs:190-287`.
