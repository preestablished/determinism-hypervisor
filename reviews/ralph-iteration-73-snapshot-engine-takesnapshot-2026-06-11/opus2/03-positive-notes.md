# Positive Notes

## P1 — The byte-determinism property is real, and the harder case works

The whole fork/dedup roadmap rests on identical state hashing to identical
refs. I tested this independently: two *separately constructed* slots+buses
(distinct KVM VMs) with the same seed/config produced the **same ref**
(`f2fddfb4…`). That means the engine's determinism doesn't just survive
re-snapshotting the same object — it survives cross-VM, which is the strict
requirement. The VCPU/section canonicalization plus the iteration-70
reserved-byte zeroing hold all the way through the engine, not just in the
unit under test.

## P2 — Section order is engine-fixed, not bus-iteration-fixed

`build_dhsnap` does not trust bus order: device sections are collected then
**sorted by `KNOWN_TAGS` position** (`snapshot_engine.rs:347-352`). I
registered a `PvBlk` in deliberately reversed base order and the container
still came out `…CLKD, PADD, BLKO, SERL`. This is the right defensive
posture — "two engines with different bus layouts produce identical bytes
for identical state" is enforced, not just hoped for.

## P3 — Ref-after-durability and post-ack clear are correctly sequenced

The dirty set is cleared *only* in the success path, *after* the store ack,
as the genuinely last step (`snapshot_engine.rs:246-249`). On any store
error the `?`/`map_err(Store)` short-circuits before the clear, so a failed
snapshot never loses dirty-tracking state — the next attempt re-ships the
same delta. This is the subtle invariant that makes retries safe, and it's
implemented exactly per §8.2.

## P4 — The entropy special-case is handled cleanly and loudly

The pv-entropy device (0x0004) is folded into `ENTR` v2
(PRNG state ‖ device regs) rather than framed alone — the resolved 6yl
landmine. The walk pulls it out of the device loop (`continue`), and a
**missing** entropy device is a loud `Codec` error
(`snapshot_engine.rs:329-330`), covered by
`missing_entropy_device_is_a_loud_codec_error`. No silent ENTR-v1 fallback,
no `BadLength{16}` trap.

## P5 — Honest, well-reasoned module documentation

The module header doesn't paper over the messy parts: it states the
hash-vs-section split decision (option (b), keep them separate), flags
ARCH §8.1's "canonical vCPU blob" wording as stale (veu divergence #8),
documents LAPC as an intentional empty-v1 placeholder, and pins
`DEVICE_BLOB_FORMAT_DHSNAP` in one place. A reviewer can reconstruct *why*
each choice was made, which is exactly what this kind of determinism-
critical seam needs.

## P6 — Joint tests exercise the production shape, end to end

The tests spawn the **real** snapshot-store in-process (R12) and reach it
through the **blocking** facade — the same sync/async bridge a vCPU worker
loop uses — rather than a mock. The incremental test even dirties pages
from *inside the guest* via a real KVM run (mov/hlt), so the dirty-ring
drain path is genuinely tested, not simulated. And the FULL test verifies
the container section-by-section (MCFG bytes, TIME boundary, ENTR seed,
VCPU re-capture equality, LAPC empty). Tests are deterministic across runs.
