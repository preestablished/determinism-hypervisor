# Critical & Important Findings

## Critical

**None.** No correctness, determinism, or safety defects found. The TX/RX
split is replay-pure, the digest convention matches ENTROPY/SDK_EVENT, the
NETL section is register-only, and writer/reader byte layouts agree exactly.

---

## Important

### I-1. No public accessor for the TX registers — the y78 loopback seam is missing

**File:** `crates/dh-devices/src/net.rs`

The module doc (lines 44–48) is explicit about the contract:

> Subscribers (run control's loopback path, y78) re-read the frame from guest
> RAM through the still-live TX regs at the very exit that rang the doorbell

But the only public methods on `PvNet` are `new()` and `apply_net_rx()`. The
TX registers `tx_buf_gpa` and `tx_len` are **private fields** with no accessor.
Compare `PvPad`, which exposes exactly the field run control needs to sample:

```rust
// pad.rs:84
pub fn frame_counter(&self) -> u32 { self.frame_counter }
```

The only way run control can currently read the TX regs is to call
`mmio_read(REG_TX_BUF_GPA, …, ctx)` and `mmio_read(REG_TX_LEN, …, ctx)`, which:
- requires synthesizing a `&mut DevCtx` purely to read state (the read path
  does not use the ctx — it is dead weight), and
- forces a byte-buffer round-trip + LE decode at the call site.

I ran `bd show determinism-hypervisor-y78`: it is **P0, OPEN, depends on mmv**,
and its job is "the run loop lands … NET_RX + AUX into DHILOG." For the loopback
wiring, run control must, at the doorbell exit, read `tx_buf_gpa`/`tx_len`, pull
the frame from guest RAM, and re-land it as a `NET_RX` record. Without an
accessor, y78 either reaches for the awkward MMIO dance or has to come back and
add the accessor anyway — modifying mmv's file in a later iteration.

**Recommendation:** add a small accessor pair now, mirroring `frame_counter()`:

```rust
/// The still-live TX descriptor at the doorbell exit — run control's
/// loopback path reads the frame from guest RAM via these (the device
/// buffers nothing). Valid only when tx_status == STATUS_OK.
pub fn tx_regs(&self) -> (u64, u32) { (self.tx_buf_gpa, self.tx_len) }
pub fn tx_status(&self) -> u32 { self.tx_status }
```

This is a one-line-of-substance change, keeps the "device buffers no frame"
invariant intact, and unblocks y78 cleanly. Marked Important (not Critical)
because the device is correct as-is and the missing seam only bites the
dependent bead.

---

### I-2. `rx_buf_gpa == 0` sentinel is undocumented as a reserved GPA and is inconsistent with pv-entropy

**File:** `crates/dh-devices/src/net.rs:123-124`

`apply_net_rx` treats `rx_buf_gpa == 0` as "no RX buffer published"
(`NetRxError::NoRxBuffer`). In this VM's GPA layout, **page 0 is real guest
RAM** — boot code lives near 0 — so GPA 0 is a legitimately addressable
buffer that the device will refuse to deliver into. The sentinel is a footgun
if a guest ever publishes a low buffer.

Two concerns:

1. **Inconsistency with pv-entropy.** `entropy.rs:122-136`'s doorbell does
   **not** special-case `buf_gpa == 0`; it calls `ctx.mem.write(self.buf_gpa,
   &bytes)` directly. With `buf_gpa == 0` and a nonzero length it would write
   into page 0 and succeed (STATUS_OK), because page 0 is backed RAM. So the
   two paravirtual devices disagree on whether GPA 0 is "unpublished" or "a
   real buffer." A reader of both files will be confused about the convention.

2. **Undocumented reservation.** If GPA-0-as-sentinel is the intended
   ARCH-level convention (the guest ABI reserves GPA 0 as "RX disabled"), that
   reservation should be stated in the module doc and ideally cross-checked
   against the §6.7 spec. Right now it is an unstated implementation choice.

**Recommendation (pick one, document it):**
- **Preferred:** keep the sentinel but document it as a deliberate ABI
  reservation ("RX_BUF_GPA = 0 means RX disabled; the guest must never publish
  a GPA-0 buffer"), and note the asymmetry with pv-entropy (entropy has no
  'disabled' state, so it needs no sentinel). A one-line doc + a comment at the
  check site closes this.
- **Alternative:** if GPA 0 must be deliverable, replace the sentinel with a
  separate `rx_enabled` flag bit (a `rx_vector != 0` style gate, or a dedicated
  register). Heavier; only worth it if §6.7 actually wants GPA-0 RX buffers.

This is Important because it is a real correctness-of-contract ambiguity at the
device ABI boundary, even though no current test or guest exercises it.
