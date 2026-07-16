# Critical and Important Findings

## Critical

None.

---

## Important

### I-1. API.md §4 NETL row says "regs + pending-RX state"; the landed section is regs-only — record the divergence in the `veu` ledger

`.agents/docs/determinism-hypervisor/API.md:623` reads:

> `NETL` | pv-net regs + pending-RX state (must be empty at snapshot; enforced)

The landed NETL section (`net.rs` `SECTION_LEN = 36`) is **registers only**. There is no
separate "pending-RX state" field — the device deliberately buffers nothing, so the
"must be empty at snapshot" rule holds *by construction* rather than by a runtime check.

This is the better design (see Positive Notes P-1), but the API.md wording now diverges
from the code in two ways:

1. It implies a distinct pending-RX state component *exists* in the section ("regs +
   pending-RX state"). It does not.
2. "enforced" implies an active check (assert/validation) at snapshot time. There is no
   such check — and none is needed, because the structure makes the state unrepresentable.

This is exactly the class of code↔doc divergence the project tracks in bead
`determinism-hypervisor-veu` (the running "divergence ledger" — it already carries
divergences #8 and #9 from iterations 73 and 84, both "code is authoritative, local copy
amended, fix upstream"). Prior iterations amended the local `.agents/docs` copy *and*
added a `veu` note so the next upstream sync does not silently revert.

**Recommended action:**
- Amend the local `API.md:623` NETL row to reflect reality, e.g.:
  `NETL | pv-net registers only (tx_buf_gpa, tx_len, tx_status, rx_buf_gpa, rx_cap, rx_len, rx_vector — 36 bytes); the "pending-RX empty at snapshot" rule (API.md §4) holds by construction because the device buffers no frame, so there is no queue to be non-empty`
- Add a divergence entry to `determinism-hypervisor-veu` recording this so the upstream
  planning tree is reconciled.

This is "Important" not "Critical" because the code is correct and the invariant the row
*cares about* is satisfied more strongly than the doc demands — but an un-ledgered
code↔doc gap in the §4 ABI table is precisely what `veu` exists to prevent.

---

### I-2. Zero-length NET_RX is accepted by the writer and the reader but rejected by `apply_net_rx` — a real (currently latent) log/device asymmetry

The canonical `NET_RX` path has an inconsistent minimum-length policy across three layers:

- **Writer** (`dhilog.rs:191-195` `net_rx`): only checks `frame.len() > MAX_NET_RX_FRAME`.
  A zero-length frame is writable.
- **Reader** (`reader.rs:534-536`): `KIND_NET_RX => payload.len() <= MAX_NET_RX_FRAME`,
  with an explicit comment: *"§3.3 gives NET_RX no lower bound: a zero-length frame is
  accepted by design."* A sealed log containing a 0-length `NET_RX` record parses and
  validates cleanly.
- **Device** (`net.rs:120` `apply_net_rx`): `if len == 0 || ... { return Err(NetRxError::FrameTooBig); }`.
  A 0-length frame is **rejected loudly** (and run control "faults the slot" per the
  `NetRxError` doc).

So a `NET_RX` record that the log format explicitly accepts and that `LogReader` happily
hands to replay would, on delivery, fault the slot. That is an asymmetry between *what the
canonical log can legally contain* and *what the device can replay* — and for a replay
system, "a legal recorded input that cannot be replayed" is a determinism/robustness hazard,
not just a cosmetic mismatch. It is **latent** today only because nothing yet emits
zero-length `NET_RX` records (the loopback producer is y78/fbr, not yet wired). But the
reader comment shows the format authors deliberately left the door open.

Note the rejection is also mis-named: a 0-length frame returns `FrameTooBig`, which is
semantically wrong (it is too *small*, not too big). See S-1.

**Recommended action (pick one, and make all three layers agree):**
- **Option A (reject everywhere):** add a lower-bound check to the writer (`net_rx`) and
  the reader (`validate_kind` `KIND_NET_RX`) so a 0-length frame is rejected at *record*
  time, making the log incapable of carrying something the device will fault on. Update
  API.md §3.3 row `0x03` to state the `>= 1` minimum. This is the safer choice for a
  deterministic system: keep the log and the device in lockstep.
- **Option B (accept everywhere):** decide a 0-length frame is a legal (if odd) delivery,
  and have `apply_net_rx` accept it (copy nothing, set `rx_len = 0`, return the vector).
  This matches the reader's stated "accepted by design" stance but means a delivery that
  writes nothing to the guest buffer.

Whichever is chosen, the three layers must agree and API.md §3.3 must state the bound.
Given the loopback producer for this path is owned by y78/fbr and not in this bead, it is
reasonable to **file this as a bead** (depending on / blocking y78) rather than expand
this device change — but it should not be left undocumented, because the reader comment
("land a minimum in API.md first if one is ever wanted") is effectively an open invitation
that this device has now silently declined to honor.
