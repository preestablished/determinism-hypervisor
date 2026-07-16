# Suggestions

### S-1. `INJECT_DEFER_BUDGET`: a const is fine, but the `arm` mode + MMIO will eventually want it tunable

`runctl.rs:29` makes the budget `const INJECT_DEFER_BUDGET: u64 = 1 << 16`. The prompt asks
whether this should live in `MachineConfig` (the margins precedent) or stay a const.

My verdict: **const is correct for now**, on three grounds: (a) it is deterministic either
way — it only bounds a *loud failure*, never the success path, so it cannot affect replay
identity; (b) `MachineConfig` already documents itself as the single source for things that
make landing *possible* (margins), and the defer budget makes *failure* possible-to-detect,
a different category; (c) 65536 single-step entries is ~tens of ms — a sane universal ceiling.

The one future force that would justify promoting it to config: once 40q wires the device bus
and the `arm` mode's MMIO-poll loop runs, a guest that legitimately spins thousands of MMIO
reads before opening a window could brush the bound. If ops ever need to raise it per-machine,
move it to `MachineConfig` then — and note in the const's doc-comment that this is the
intended migration trigger. (Today: leave it. Just add the migration breadcrumb.)

### S-2. `arm` mode is an MMIO-exit storm against the §10 ~3k-exits/s envelope — call it out for 40q

`timer_guest.asm:100-103` `.wait` loop polls `CLOCK_VNS` via MMIO every iteration with no
back-off:

```asm
.wait:
    mov rcx, [rbx + CLOCK_VNS]   ; MMIO read -> a foreign exit once the bus is live
    cmp rcx, rax
    jb  .wait
```

Once 40q makes `CLOCK_BASE` a real MMIO region, *every* `.wait` iteration is a VM exit. At a
1ms arm period over 10s that is a tight spin between deadlines — potentially far over the §10
~3k-exits/s budget the architecture sets for the device loop. The asm header already warns the
mode "REQUIRES the device-bus run loop (bead 40q)", which is good, but it does not flag the
*exit-rate* hazard.

The design tension is real and worth recording: the guest cannot `hlt`/`pause` between polls
without a timer interrupt to wake it — which is the very thing M3 is bootstrapping. So `arm`
mode as written will be an exit storm. **Action:** file/annotate a 40q note that `arm` mode
needs either (a) a `pause`/`monitor-mwait`-style throttle the host can satisfy, or (b) the host
synthesizing the wait by advancing vns and re-entering only at the deadline boundary (so the
spin never actually executes thousands of times under the deterministic clock). This is a
40q-shaped design question, not an iter-40 defect — but iter-40 is where the guest landed, so
flag it here.

### S-3. The masked-defer test burns the full 65536-step budget on every run

`masked_variant_defers_forever_live` (`runctl.rs`) drives the deferral to exhaustion: 65536
single-step VM entries each run. That is the dominant cost in the 2.6s the three idt tests
take. It is correct and deterministic, but if the budget is ever raised (S-1) this test's
runtime scales linearly. Consider asserting the failure with a *smaller* explicit budget by
exposing a test-only knob, or document that this test's cost tracks `INJECT_DEFER_BUDGET`.
Not urgent — 65536 steps is still sub-second.

### S-4. `read_table` reads `count` bytes without bounding against the table region

`read_table` (test helper, `runctl.rs`) reads `count` bytes after the 8-byte header
(`vec![0u8; count as usize]`). If a future guest bug wrote a huge `count`, the helper would try
a large allocation / over-read. It is test-only and the guest is trusted, so this is cosmetic,
but a `count.min(SOME_CAP)` guard would make the helper robust to a misbehaving guest under a
new test. Optional.

### S-5. Gate attribute byte: consider documenting the `0x8E00` decode inline

`SETGATE` writes `0x8E00` at offset 4 (`timer_guest.asm:38`) with the comment "P=1 DPL=0
interrupt gate, IST=0". For the next reader, the full decode is: byte at +5 = `0x8E` =
P(1)·DPL(00)·0·type(0xE = 64-bit interrupt gate); byte at +4 = `0x00` = IST(000) + reserved.
The comment is accurate; a one-word add ("type=0xE") would make it grep-complete. Trivial.
