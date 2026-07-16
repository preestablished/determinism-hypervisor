# Critical & Important Findings

## Critical

None. The test is sound and passes live and repeatably.

---

## Important

### I-1. Run-twice comparison omits device-internal state AND drained beacons — it leans entirely on the full-RAM hash to catch device divergence

The repeatability assertion compares exactly this tuple:

```rust
(out.serial, out.icount, out.state_hash, out.log_records)
==
(out2.serial, out2.icount, out2.state_hash, out2.log_records)
```

What `state_hash` actually covers, traced through `runctl.rs` → `hash.rs`:

- `run_segment` calls `seg.chain.push_final_link(seg.slot, &[], icount, vns)` — note the
  **`&[]` device-sections argument** at every call site (the epoch path, the final-stop
  path, the pause path, and `finish_halted`). So `device_sections` is ALWAYS empty in
  this path; `hash.rs::device_sections(bus)` is never invoked here.
- With `epoch_len = DEFAULT_EPOCH_LEN = 50_000_000` and `Until::IcountBudget(10_000_000)`,
  no epoch boundary fires; the guest HLTs a few thousand instructions in. So the chain is
  H_0 then exactly ONE final link at the halt boundary, covering: the canonical vCPU blob
  + the FULL guest-RAM walk + icount + vns. Device sections: **not included.**

Therefore the run-twice comparison's blind spots are precisely:

1. **pv-entropy PRNG `word_pos`/stream** — out of the hash (the PRNG lives in
   `DetEntropy`, not RAM; only the *digest* of served bytes goes to the log, and
   `log_records` is just a count). The served *bytes* land in guest RAM (`ent_buf`),
   so RAM does cover the entropy *output* — but not the generator's advanced position.
2. **pv-pad latch / frame_counter** — host device state, out of the hash. (The guest
   never reads the latch into a RAM location it keeps; `'P'` reads PAD0 into `eax` and
   discards it.) The host-injected `0x0000_A11B` latch is invisible to the comparison.
3. **detchannel host state** — `init_lo/hi/status`, `channel_gpa`, producer seqs,
   `inject_iseq`, `last_quiesce_ack`, metrics. All out of the hash and out of every
   assertion. (The channel page itself is guest RAM, so the ring bytes ARE hashed.)
4. **pv-blk overlay** — the guest's sector-0 write lands in the `overlay` HashMap, NOT
   guest RAM and NOT the hash. The read-back lands in `blk_rbuf` (guest RAM, hashed),
   so the *result* is covered, but the overlay dirty-cluster state is not.
5. **The drained `beacons` Vec** — its CONTENTS are checked once (run 1: exactly one
   Beacon, id 0xB33F) but are NOT part of the run-twice tuple. Two runs that differed in
   the drained event's `vnanos`, `seq`, `ring`, or even payload would still pass the
   repeatability assertion. (The `vnanos` field is `vns_sample`, set at the stage-C clock
   read; it is also written into guest RAM at ringW+8, so the RAM hash *does* incidentally
   cover the in-RAM copy of vnanos — but not the host-side drained struct.)

This is "works for this guest by luck of layout," not "the acceptance net checks device
determinism." For the named M1 milestone ("the WHOLE run is bit-identically repeatable")
the comparison should be made to mean what it says.

Recommended minimal-but-strongest addition (implementable — both `bus` and `channel` are
live after the run; the closure borrows them by `&mut`, so capture a fingerprint into
`RunOutcome` right after `run_segment` returns):

```rust
// after run_segment(...) returns, before dropping borrows:
let dev_fp = dh_vmm::hash::device_sections(&bus);   // pv-clock/pad/entropy/blk sections
let mut chan_fp = Vec::new();
channel.snapshot(&mut chan_fp);                     // EVTC bytes: init/gpa/seqs/iseq/ack
```

Then compare `(serial, icount, state_hash, log_records, dev_fp, chan_fp, beacons)` across
the two runs. `device_sections` already exists and is deterministic (its own unit test
proves byte-identity); `DetChannelHost::snapshot` is a stable LE layout. Adding `beacons`
to the compared tuple is free — `GuestEvent` derives `PartialEq` (the detchannel tests
already `assert_eq!(events_a, events_b)`). This converts the milestone claim from
"true for this RAM layout" to "true for the observable device surface."

Severity rationale: Important not Critical because the run as written IS deterministic
and the assertion as written DOES pass; the gap is in coverage strength, and an M1
*acceptance* test is exactly where that coverage should be tightened before M2 builds on
top of it.

### I-2. The IRQ queue is threaded through every dispatch but never drained, applied, or asserted-empty — lock it down

`run_m1` allocates `let mut irqs = Vec::new();`, hands `irqs_r` to every `DevCtx::new`,
and then... nothing. The queue is never drained into `Segment.injections`, never applied,
and never inspected after the run.

For THIS guest that is correct behavior, and I verified each device's IRQ path:

- **pv-entropy**: `doorbell()` never calls `request_irq`. (read entropy.rs)
- **pv-pad**: edges fire only through `apply_pad_set` returning `Option<u8>`, which the
  test calls once host-side (`pad.apply_pad_set(0, 0x0000_A11B)`) and *discards the
  return* before registering the device — so no edge IRQ is queued, and the MMIO read
  path never queues one. (read pad.rs)
- **pv-clock**: the timer fires via the agenda (`Segment.timer`), not via `ctx`; the test
  passes `timer: None`. (read clock.rs / runctl.rs)
- **detchannel**: `pio_out`/`drain` never call `request_irq`. (read detchannel.rs)

So `irqs` provably stays empty here. And injection would be *wrong* for this guest anyway:
`device_exercise.asm` never executes `sti` or `lidt` — IF is clear and there is no IDT, so
any injected vector would fault into a zero IDT. The design (devices queue, the boundary
engine drains and applies the §3.4 rule) is right; this guest simply has no IRQ inputs.

The defect is that the acceptance test *silently relies* on the queue staying empty
without saying so. If a future device edit (or a future guest) started queuing an IRQ
here, it would be **silently dropped** and the test would still pass green — masking a
real determinism/correctness regression. Add one line after the run:

```rust
assert!(irqs.is_empty(), "device_exercise queues no IRQs; a queued-but-dropped IRQ is a bug");
```

This costs nothing, documents the invariant, and turns a silent-drop into a loud failure.
