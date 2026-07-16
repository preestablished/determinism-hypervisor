# Critical & Important Findings

## Critical

**None.** I traced every slice index in `Container::parse` to a dominating
bounds check and could not construct a panicking input (analysis in
`03-positive-notes.md`). Spec fidelity, device-id cross-checks, and typed
section layouts are all correct.

---

## Important

Both Important items are **documentation / tracking gaps**, not code defects.
The codec itself is correct; these are about making the deferred semantic
decisions impossible to lose. Neither blocks merge of bead 68l.

### I-1 — The ENTR device/section conflict is real but only obliquely documented

**File:** `crates/dh-snapshot/src/dhsnap.rs:76`, `:310-314`
**Severity:** Important (latent correctness trap for a future bead)

The device-id map routes the entropy device to the ENTR tag:

```rust
0x0004 => Some(tag::ENTR), // pv-entropy (PRNG state; see EntrSection)
```

But the two ends of that arrow disagree on what ENTR *contains*:

- **`EntrSection`** (this file, and API.md §4) is the **56-byte ChaCha20 PRNG
  state** `seed[32] + stream u64 + word_pos u128`.
- **`PvEntropy::snapshot`** (`crates/dh-devices/src/entropy.rs:171-175`,
  `SECTION_LEN = 16`) emits the **16-byte MMIO register file**
  `buf_gpa u64 + len u32 + status u32` — *not* the PRNG state at all. The PRNG
  state lives in `DetEntropy` (`ctx.entropy`), reachable via
  `EntropyState`/`.state()`, which the device's `DetDevice::snapshot` never
  touches.

So if a future snapshot-engine bead naively wires "device 0x0004 ⇒ write its
`DetDevice::snapshot` bytes under the tag from `tag_for_device_id`", it will
emit a 16-byte body under ENTR, and this very codec's `EntrSection::decode`
will reject it with `BadLength { found: 16 }`. The two sources of truth for
"what is an ENTR section" are inconsistent, and nothing in the tree makes that
contradiction loud.

The in-code comment at `:314` mentions "integration is bead 6yl" but does so as
a passing aside on `EntrSection`, and the map comment at `:76` says only
"PRNG state; see EntrSection" — neither states that the entropy *device's* own
snapshot bytes are a different shape and must be reconciled. A reader scanning
the map will reasonably assume the device and the section agree.

**This is correctly out of scope for bead 68l** — the codec owns FRAMING and
typing, not who fills ENTR — but the conflict must be pinned so 6yl cannot miss
it.

**Fix:** make the contradiction explicit at the map entry and ensure bead 6yl
carries it. Suggested comment:

```rust
// pv-entropy. NOTE: ENTR's section body is the 56-byte ChaCha20 PRNG state
// (EntrSection), NOT PvEntropy::DetDevice::snapshot's 16-byte MMIO regs
// {buf_gpa,len,status}. The engine must source ENTR from ctx.entropy.state()
// (dh-devices EntropyState), not from the device's snapshot(). Reconciling
// who emits ENTR is bead 6yl (M4 integration) — do not auto-route 0x0004's
// device snapshot bytes here.
0x0004 => Some(tag::ENTR),
```

And confirm bead 6yl's description records: "ENTR body = EntropyState (56B),
sourced from DetEntropy, not PvEntropy::snapshot (16B MMIO regs)."

### I-2 — `0x0007 → NETL` is anticipated; ensure the bead-mmv dependency is recorded

**File:** `crates/dh-snapshot/src/dhsnap.rs:79`
**Severity:** Important (forward-reference to not-yet-landed device)

```rust
0x0007 => Some(tag::NETL), // pv-net loopback (bead mmv)
```

Unlike 0x0001–0x0006, there is **no `DEVICE_ID_PV_NET = 0x0007` constant in the
tree yet** — pv-net lands with bead mmv (confirmed: `grep DEVICE_ID` over
`crates/dh-devices/src` returns clk/pad/entropy/blk/serial + detchannel's
0x0001, but no 0x0007). The mapping is **safe**: it returns `Some(NETL)` for an
id no live device claims, and the empty-section/pending-RX-empty rules are
already exercised by the test battery (`NETL` pushed with `&[]`). There is no
collision risk because 0x0007 is otherwise unassigned.

The concern is purely that this anticipated id is asserted in one place
(`dhsnap.rs`) but the authoritative constant will be born in another (`pv-net`,
bead mmv). When mmv lands, `DEVICE_ID_PV_NET` **must equal 0x0007** or the map
and the device silently diverge.

**Fix:** record the cross-reference so mmv can't pick a different id. Either:
- a one-line note on bead mmv: "pv-net `DEVICE_ID_PV_NET` MUST be `0x0007` to
  match `dhsnap::tag_for_device_id`'s anticipated NETL entry", and/or
- when mmv lands, add a compile-time assertion in dh-devices or a test mirroring
  `device_id_tag_map_is_total_over_known_devices` that imports the real
  `DEVICE_ID_PV_NET` and asserts `tag_for_device_id(DEVICE_ID_PV_NET) ==
  Some(tag::NETL)` (today's test hardcodes `0x0007`, which won't catch a future
  constant drift).
