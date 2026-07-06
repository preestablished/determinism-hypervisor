# Handoff resolution

## Localized cause

The remaining real M9 no-frame observable is localized to the
reference-workload harness wiring, not to determinism-hypervisor frame-budget
or detchannel replay machinery.

Evidence from reference-workload commit
`7e94a828b2b9d252cff511cef5fc8baa4836caca`:

- `crates/refwork-harness/src/main.rs` runs the frame loop with
  `NoopPlatform`;
- `crates/refwork-harness/src/frame.rs` defines `NoopPlatform::frame_mark` as
  an empty method;
- `NoopPlatform::poll_input` and `NoopPlatform::quiesce_check` are also empty;
- the only non-empty `Platform::frame_mark` implementation in the harness
  crate is test-only.

That explains the hypervisor evidence:

- the real initramfs reaches `Ready` and logs `boot: start-sent`;
- after Ready, `Run{frame_budget}` hard-caps with pv-pad frame counter still
  `0`;
- replacing the game image with the NOP ROM does not change the outcome;
- no post-Ready detchannel `FrameMark` events are buffered.

The harness can run emulator frames internally while publishing no
`detguest_sdk::frame_mark()` records and writing no pv-pad `FRAME_COUNTER`
boundaries.

## Owning follow-up

Filed in the owning repo:

- `reference-workload`: `refwork-4qj` — Wire refwork-harness production
  platform to detguest-sdk frame APIs.

Acceptance for that follow-up is that production `refwork-harness` uses a
detguest-backed platform after `Start`:

- `poll_input` reads `detguest_sdk::poll_input(0)`;
- `frame_mark` calls `detguest_sdk::frame_mark()` once per completed emulator
  frame;
- `quiesce_check` calls the SDK quiesce path;
- the real M9 Linux run under determinism-hypervisor no-tick reaches
  `Run{frame_budget}=BudgetReached` with post-Ready `FrameMark` events.

## Determinism-hypervisor status

The determinism-hypervisor branch still carries the necessary repo-local fixes:

- SDK-normal no-doorbell detchannel frame-boundary drain in service and replay;
- replay final-link handling for regenerated terminal frame-boundary records;
- real-emulator provenance checks that reject stale synthetic initramfs;
- M9 diagnostics that show frame counter and buffered event evidence.

No additional determinism-hypervisor code change is indicated by this
localization.
