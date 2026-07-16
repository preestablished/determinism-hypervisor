# Critical & Important Findings

---

## CRITICAL — C1: `drain_net_tx` mints a loopback frame for a *faulted* TX (status-unchecked) and allocates from an unvalidated `u32` length

**File:** `crates/dh-vmm/src/recording.rs:248-263`
**Devices contract:** `crates/dh-devices/src/net.rs:100-123, 183-200`

### The code

```rust
pub fn drain_net_tx(&mut self) -> Result<Option<Vec<u8>>, RecordError> {
    let (gpa, len) = {
        let net: &mut PvNet = self.device_mut(DEVICE_ID_PV_NET, "pv-net")?;
        net.tx_regs()
    };
    if len == 0 {
        return Ok(None);
    }
    let mut frame = vec![0u8; len as usize];        // (A) unbounded alloc
    if self.mem.read(gpa, &mut frame).is_err() {
        return Ok(None); // the doorbell already faulted; nothing sent
    }
    Ok(Some(frame))
}
```

### Why it is wrong

`tx_regs()` returns `(self.tx_buf_gpa, self.tx_len)` — the **last values the guest programmed**, with **no reference to `tx_status`** and **no cap**. The device's own doorbell logic (`net.rs:100-113`) is the only place that decides a TX is valid:

```rust
fn doorbell(&mut self, ctx: &mut DevCtx) {
    if self.tx_len == 0 || self.tx_len > MAX_FRAME {   // oversize => FAULT
        self.tx_status = STATUS_FAULT;
        return;                                         // NO NET_TX record logged
    }
    let mut frame = vec![0u8; self.tx_len as usize];
    if ctx.mem.read(self.tx_buf_gpa, &mut frame).is_err() {
        self.tx_status = STATUS_FAULT;                  // NO NET_TX record logged
        return;
    }
    ctx.log_net_tx(self.tx_len, digest8);               // <-- the ONLY NET_TX emit
    self.tx_status = STATUS_OK;
}
```

So a guest can:

1. Program `REG_TX_LEN = 4000` (> `MAX_FRAME = 2048`) and `REG_TX_BUF_GPA` to a valid page.
2. Ring `REG_TX_DOORBELL` → doorbell sees `tx_len > MAX_FRAME`, sets `STATUS_FAULT`, logs **no** NET_TX record.
3. The exit returns to run control, which (in the loopback config this method is built for) calls `drain_net_tx`.
4. `drain_net_tx` reads `len = 4000` (uncapped!), allocates a 4000-byte frame, `mem.read` of a valid 4000-byte page **succeeds**, and returns `Some(frame)`.
5. The loopback caller lands that frame back via `apply_net_rx` → **a canonical NET_RX record exists for a frame the device's NET_TX path explicitly rejected and never recorded.**

Two distinct failures:

- **Replay divergence (the serious one):** the record stream now contains a NET_RX with no antecedent NET_TX. The doc on `net.rs:21-23` states the contract run control inherits: *"TX_STATUS is STICKY ... TX frames must be drained PER EXIT via `tx_regs`"* — but it never says "only when STATUS_OK", and this code honors neither status nor cap. A faulted doorbell followed by a drain mints input out of thin air. Record and replay can diverge because the doorbell-fault path is the kind of thing a replay's register-state reconstruction may or may not reproduce identically, and the NET_RX it produces is not anchored to any canonical TX event.
- **DoS / divergent allocation (A):** `tx_len` is a raw guest-written `u32` (`net.rs:186` accepts any value), so `vec![0u8; len as usize]` is a guest-controlled allocation up to **4 GiB**. `doorbell()` caps at `MAX_FRAME` *before* it allocates; `drain_net_tx` does not. The `mem.read` will usually fail for a 4 GiB range (returning `Ok(None)`, masking it), but for any `MAX_FRAME < len <= guest_ram_size` the allocation is real and the frame is minted.

### The fix

`drain_net_tx` must consult `tx_status` and only drain `STATUS_OK`, and must cap `len` at `MAX_FRAME`. The cleanest fix is to expose status from the device and gate on it (and have `tx_regs`/a new accessor return the status, or add `pub fn tx_status(&self) -> u32`):

```rust
let (gpa, len, status) = { let net = self.device_mut::<PvNet>(...)?; net.tx_state() };
if status != STATUS_OK || len == 0 {
    return Ok(None);          // faulted or idle doorbell — nothing was sent
}
debug_assert!(len <= MAX_FRAME); // doorbell guarantees this when STATUS_OK
let mut frame = vec![0u8; len as usize];
```

Gating on `STATUS_OK` also makes the `len <= MAX_FRAME` cap automatic (the doorbell only reaches `STATUS_OK` for valid lengths), and removes the 4 GiB-alloc path. The current `Ok(None)` on `mem.read` failure becomes an unreachable belt-and-suspenders for the `STATUS_OK` case (a frame that read cleanly at the doorbell should still read cleanly at the same exit), and you could even upgrade it to a loud error there.

### Severity rationale (honest)

This is **latent**: nothing in the tree calls `drain_net_tx` yet (the loopback caller is bead **czq**, OPEN). So it cannot fire on `main` today and does **not block y78's merge**. But it is a P0-class divergence bug the moment the loopback path is wired, and the fix is small and local. File it now so it lands with — or before — czq. I'd rate it Critical-when-wired / High-priority-to-fix-now.

---

## IMPORTANT — I1: No EPOCH_HASH *writer* exists — a5e cannot start

**Evidence:**
- `grep` for `epoch_hash` / `KIND_EPOCH_HASH` across `crates/` finds: the kind constant (`dhilog.rs:48`), the flag bit (`dhilog.rs:39`), the reader-side decode (`reader.rs` `RecordBody::EpochHash`, validation `reader.rs:485,519,539`), and the *consumer* beads (1py VerifyReplay, a5e acceptance). **No `LogWriter::epoch_hash()` method exists** — `LogWriter`'s public record methods are `pad_set`, `dev_event`, `net_rx`, `pio_answer`, `frame_mark`, `seal` (and entropy/net_tx/timer/sdk via ctx). Nothing logs EPOCH_HASH into a DHILOG.

### Why it matters for a5e

`bd show a5e`: *"replay reproduces end_state_hash AND **every EPOCH_HASH**."* The product's core property (1py: *"checks EPOCH_HASH records against the live chain as it goes"*) requires the record stream to *contain* EPOCH_HASH records. The recording side currently produces a sealed log with PAD_SET/NET_RX canonical records + FRAME_MARK aux + END — but **zero EPOCH_HASH records**. A replay of such a log has nothing to compare; a5e's "every EPOCH_HASH equal" assertion is vacuous (or fails the `FLAG_EPOCH_HASHES` consistency check, depending on how the flag is set).

### Whose job is this?

It is correctly **out of y78's scope** — y78's bead text is "PAD_SET/DEV_EVENT/NET_RX + AUX", and EPOCH_HASH is a periodic-chain-hash AUX record that needs a *cadence* (per epoch / per N icounts), which is a run-loop concern, not a device-input concern. But it is the **immediate next gap on the a5e critical path**, and I found **no bead that owns the producer**. The closest beads all *consume* epoch hashes (1py, a5e, pee). 

### Action

File a P0 bead: *"DHILOG EPOCH_HASH writer: `LogWriter::epoch_hash()` + run_segment cadence that logs the live `StateHashChain` epoch hash at each epoch boundary"* and make a5e depend on it (alongside 39w). Without it, a5e is blocked even after 39w (replay) lands.

---

## IMPORTANT — I2: Module doc "Build it fresh per segment" contradicts the live test (which reuses one rail across 3 segments) — terminology drift

**File:** `crates/dh-vmm/src/recording.rs:64-67` (doc) vs `recording.rs:~430-490` (live test).

### The contradiction

The `DeviceRail` doc says:

> *"One segment's device rail. **Build it fresh per segment** (the LogWriter carries the §3.1 segment header), feed `service_exit` to `run_segment`'s on_exit ... then `seal` with the outcome."*

But `pad_echo_live_run_records_inputs_frame_marks_and_seals` builds **one** `DeviceRail` (one `LogWriter`) and runs it across **three** `run_segment` calls (`run_one` ×3), applying PAD_SET between them, then seals **once** at the end. One DHILOG spans all three `run_segment` invocations.

### Which is right?

The **test is right for M5**, and the **doc is using "segment" in two different senses** — that is the drift. Reconciling against a5e:

- a5e wants *"one log across a 60s-vns run with many segments"*. A DHILOG (one `LogWriter` lifetime, one §3.1 header, one seal) covers a **recording span between snapshots**, which is composed of **many `run_segment` calls** (each `run_segment` is a scheduling quantum bounded by `Until::IcountBudget`). The PAD_SET inputs land *between* `run_segment` calls, at the boundary icounts those calls return.
- The doc's word "segment" conflates the `run_segment` quantum with the DHILOG span. The DHILOG lifetime is the **snapshot-to-snapshot span**, not the `run_segment` quantum. So "build fresh per segment" is misleading if "segment" reads as "per `run_segment` call" — that would force a new log (and a new header/seal) every quantum, which is exactly *not* what M5 needs and *not* what the test does.

### Fix

This is a doc-only fix but it matters because the rail is the thing a5e/ol1 will own. Change the doc to say the rail's LogWriter lifetime is the **recording span (snapshot-to-snapshot), spanning many `run_segment` calls**, and seal once at span end. Define the term once at the top of the module so "segment" stops meaning two things. The `seal(self, ...)` consuming-self signature is already correct for "one seal per log" — it's only the prose that drifts.
