# Suggestions

## S-1 — Assert the EXACT record count (5), not just `>= 4`

I instrumented the test: it produces **exactly 5** DHILOG records, bit-stable across 6 runs:
1. ENTROPY (pv-entropy doorbell `log_entropy`)
2. PIO_ANSWER for `IN 0xD37C` (CHANNEL_INIT status)
3. DEV_EVENT/CONS_BUMP (ring-W drain of the one beacon)
4. SDK_EVENT (the beacon's digest)
5. PIO_ANSWER for `IN 0xD380` (doorbell defined-0)

The `>= 4` floor is loose enough to silently absorb a regression that drops one record (e.g. the entropy AUX vanishing, or the SDK_EVENT digest failing). Pin it: `assert_eq!(out.log_records, 5)`. If a future guest-surface change is expected to move it, update the constant in the same commit — that is the point of an acceptance test. The exact icount (739) is also stable and could be asserted, though icount is more host-microarchitecture-sensitive than the record count, so the run-twice equality already covers it; the record count is the cheap, portable pin.

## S-2 — Fold `bus.devices()` snapshots into the final fingerprint (see IMPORTANT-1)

After `run_segment` returns, the test still owns `bus`, `entropy`, and `channel`. Capture `dh_vmm::hash::device_sections(&bus)` (and optionally `channel.snapshot(..)` + `entropy.state()`) into `RunOutcome`, and compare them between the two runs. This turns "device state is deterministic" from an inference into an assertion and pre-stages the M3 replay comparison.

## S-3 — `boundary_rip = 0` in DevCtx is benign for M1 but should be a tracked debt bead

Every record this test emits (ENTROPY, PIO_ANSWER, CONS_BUMP, SDK_EVENT) carries `boundary_rip = 0` because the rip is not retrievable inside `on_exit` (the vCPU is mutably borrowed by the segment; the comment at m1_acceptance.rs:193 is accurate). For M1 this is fine: BOTH record and (future) replay stamp 0, so the rip field never participates in a divergence. But M3 replay fidelity may want the real rip for diagnostics/anchoring, and a silent 0 across the whole canonical log is the kind of thing that's invisible until it isn't. File a bead: "DevCtx.boundary_rip is 0 on the device-loop path; thread the landed Boundary.rip into on_exit before M3 replay compares rip-bearing records." Not blocking.

## S-4 — Make the `DETCALL_HI` bound match the SDK's declared range

The test scans `(0xD370..0xD3A0)` for detcall PIO. `detguest_wire::ports` declares `PORT_RANGE_END = 0xD39F` (inclusive), i.e. the range is `0xD370..=0xD39F` = `0xD370..0xD3A0`. So the test's exclusive `0xD3A0` is correct, but it hardcodes the literal instead of deriving from `PORT_RANGE_START`/`PORT_RANGE_END`. Import and use those constants so the window can't drift from the ABI owner. (The asm only goes up to 0xD380, so this is hygiene, not a live bug.)

## S-5 — Add an independent-fetch invariance test for the CPUID hash (supports CRITICAL-1)

The existing `cpuid_table_hash` self-equality test (`assert_eq!(hash(&masked), hash(&masked))`) hashes the SAME `CpuId` object twice and can never catch host-variable content. Add: call `masked_cpuid(&kvm)` twice (two independent ioctls) and assert the hashes match. With CRITICAL-1 unfixed this test FAILS, which is exactly the signal that's currently missing from CI.
