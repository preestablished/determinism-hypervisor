# Implementation plan

## 1. Preserve and correct the public detchannel regression

Keep the existing explicit-doorbell fixture because it proves the detcall
doorbell seam, but add or alter coverage so dh-worker also proves the SDK's
normal frame path:

- publish a ring-W `FrameMark`;
- do not ring `DOORBELL_RING_W` on the non-full path;
- write pv-pad `FRAME_COUNTER`;
- require `StreamGuestEvents(FrameMark)` to report strict per-frame events with
  icounts before or at the matching frame-counter boundaries.

The production change expected by this test is to drain detchannel at
`FRAME_COUNTER` MMIO exits in `dh-worker`, matching guest-sdk's harness.

The drain must be limited to a successful 4-byte write to
`PV_PAD_BASE + REG_FRAME_COUNTER`. The service path should apply the pv-pad
write first, then drain detchannel at the same boundary and return those events
to both `StreamGuestEvents` retention and `NextSdkEvent` matching. Replay must
mirror the same branch and ordering, or recorded `SDK_EVENT` positions will not
replay consistently.

## 2. Add real-emulator artifact provenance checks

The worker M9 acceptance path should reject the stale synthetic contract when a
test claims to exercise the real emulator:

- initramfs contains `/usr/bin/refwork-harness`;
- initramfs does not contain `/opt/m9-refwork-contract`;
- `boot.toml` autostart exec is `/usr/bin/refwork-harness`;
- `[unit.control]` has `protocol = "refwork-ctl"`, `proto_version = 1`,
  `game_dev = "/dev/vdb"`, and `game_source = "pv-blk"`;
- diagnostics print initramfs hash/size and game hash/size.

This can live in `crates/dh-worker/tests/common/mod.rs` for worker gates. The
existing host-only determinism fixture contract can remain broader if other
test targets still use it, but the real-emulator frame-budget gate must be
strict.

## 3. Add controlled game-variable worker coverage

Add an ignored M9 diagnostic test that uses the real-emulator initramfs but
replaces the pv-blk game image with a minimal NOP ROM in the worker image
cache.

Expected outcome:

- if real emulator + NOP game frames under dh-worker, the game/content path is
  implicated and must be isolated further;
- if real emulator + NOP game also hard-caps, continue worker localization.

This test should print enough evidence to compare with the real-game red case:
Ready icount, first run stop reason, frame counter, streamed frame events, and
any post-Ready guest events.

## 4. Improve the red real-game diagnostics

When `linux_m5_frame_budget_records_post_ready_frame_marks` fails before the
first frame, report:

- exact initramfs identity and autostart exec;
- game image hash and size;
- Ready icount and Ready frame counter;
- post-run stop reason, icount, and frame counter;
- buffered guest event count/tail after pause drain.

The test can stay ignored and red for the real game until ownership is known,
but its failure must name the missing variable instead of only reporting
`reason 4`.

## 5. Do not over-close the external observable

If controlled evidence shows the real game does not frame in guest-sdk either,
record that in the resolution and file follow-up work for reference-workload or
guest-sdk. The determinism-hypervisor branch should still land the worker drain
semantics and provenance/diagnostic gates, because those are real repo-local
defects exposed by the request.
