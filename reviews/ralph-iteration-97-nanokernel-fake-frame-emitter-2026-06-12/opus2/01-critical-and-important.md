# Critical and Important Findings

**None.**

No Critical and no Important issues were found in this branch. The change is a
small, additive test guest that closely follows the accepted `pad_echo` pattern.
Below I record the probes I ran specifically to *rule out* the plausible
correctness traps for this kind of guest, so the absence of findings is auditable
rather than asserted.

## Ruled out: the boot MMIO read perturbing M5 acceptance

The boot-time `mov r10d, [r8 + REG_FRAME]` is an MMIO-read VM exit that occurs
**before** the `'G'` OUT and before the first `FRAME_COUNTER` write. The concern
is whether this extra exit shifts anything the M5 acceptance keys off.

Verified against `crates/dh-vmm/src/runctl.rs:344-352`: `frame_budget` counts
exits where `VcpuExit::MmioWrite(gpa, _)` with `gpa == frame_mark_gpa`
(`PV_PAD_BASE + REG_FRAME_COUNTER`). The boot read is a `MmioRead`, not a
`MmioWrite`, so it does **not** increment `frames_seen`. Frame-boundary counting
is unaffected. The read does add one fixed-icount exit before the loop, so every
*absolute* frame-boundary icount shifts by a constant relative to a hypothetical
zero-init guest — but the harness reads boundary icounts from the per-segment
FRAME_MARK table (ARCH §6.6), and record/replay see the identical instruction
stream, so the table is reproduced bit-identically. No perturbation. **Not an
issue.**

## Ruled out: §6.6 ring-W FrameMark equality faulting a channelless guest

ARCH §6.6 states the ring-W `FrameMark` index MUST equal the written
`FRAME_COUNTER` or the slot is `FAULTED`. `fake_frames` issues no detcall and
never runs CHANNEL_INIT, so it has no ring W. The equality rule is a run-control /
detchannel-drain contract check that only has an operand when a ring-W FrameMark
event is present; `pad.rs::log_frame_mark` (the device side) merely logs the AUX
record and cannot fault. The already-accepted `pad_echo` guest is likewise
channelless and is not faulted. **Not an issue** for this guest.

## Ruled out: u32 FRAME_COUNTER wrap

The first bump writes `F+1`; strict increase holds per bump. `pad.rs` /
ARCH §6.4 describe the counter as strictly increasing along a lineage. `u32`
wraps at `0xFFFFFFFF`; nothing in `pad.rs` detects wrap. At ~`PACE_ITERS×7 + ~3`
≈ 500 instructions/frame, reaching `2^32` frames is ~`2e12` instructions —
practically unreachable in any acceptance run. This is at most a half-line
comment (see suggestions), **not** a correctness defect.

## Ruled out: drift-pin robustness / vacuous pass

The pin strips comments (`l.split(';').next()`) before matching, so the doc
header's mention of `REG_FRAME` cannot satisfy it. The positional check
`t.starts_with("mov") && t.contains("r10d, [r8 + REG_FRAME]")` matches the actual
post-trim line `mov     r10d, [r8 + REG_FRAME]` (leading whitespace trimmed; the
internal `mov`→operand gap is tolerated by `starts_with`; the bracket-expression
substring matches the source spacing exactly). A register rename
(`r10d`→`r9d`) or a reformat of the bracket spacing would make `contains` fail,
so the pin `expect()`s and the test fails **loudly** — the intended behavior. It
cannot pass vacuously. **Not an issue.**
