# Positive Notes

### P-1. The "buffer nothing" design makes the §4 empty-pending-RX rule unbreakable

The central design decision — the doorbell logs the AUX `NET_TX` digest and stores no
frame, leaving subscribers to re-read guest RAM through the still-live TX registers at the
doorbell exit — is genuinely elegant. It turns API.md §4's "pending-RX state must be empty
at snapshot; enforced" from a runtime *check* (which could have bugs, race windows, or
forgotten code paths) into a *structural guarantee*: there is no queue, so there is nothing
that can be non-empty. This is the right way to satisfy a "must be empty" invariant — make
the non-empty state unrepresentable. The NETL section being 36 bytes of pure registers
falls directly out of this, and the snapshot/restore code is correspondingly trivial and
obviously correct.

### P-2. Tight pattern fidelity with the established devices

`apply_net_rx` is a faithful sibling of `PvPad::apply_pad_set`: identical
`Result<Option<u8>, _>` signature, identical "0 = disabled, else inject" vector semantics,
and identical `& 0xFF` masking applied *on the write path* (`net.rs:169`) so that read-back,
snapshot bytes, and the injected vector can never disagree. The TX side mirrors `PvBlk`'s
one-deep doorbell-with-sticky-STATUS model. A reviewer who knows pad and blk needs almost
no ramp-up to audit this device, which is exactly the property a deterministic device
family wants.

### P-3. The digest test exercises the real sealed-log path, not a shortcut

`tx_doorbell_logs_the_aux_net_tx_digest_and_completes_ok` (`net.rs:240`) does not just
assert on an in-memory writer field — it seals the log, re-parses it with the real
`LogReader`, filters for `RecordBody::NetTx`, and asserts both `len == 64` and
`digest8 == LogWriter::digest8(&frame)` against the exact guest bytes. This proves the
full ABI round-trip (writer payload layout ↔ reader's `u32at(0)`/`u64at(8)` decode ↔ the
16-byte `_pad`-gapped shape) end to end, and would catch a payload-offset regression that a
field-only assertion would miss. This is the highest-value test in the file and is not
tautological.

### P-4. Faults are loud and provably log nothing

`tx_faults_are_loud_and_logged_nothing` covers all three TX fault paths (zero length,
oversize, unmapped buffer) and the OK test additionally asserts `c.log_fault().is_none()`,
confirming the happy path does not silently swallow a log-write failure. `apply_net_rx`'s
rejection test covers no-buffer, over-cap, empty, and mem-fault. Coverage of the device's
error surface is thorough.

### P-5. Deny-list purity holds, and the gate covers the new module for free

The `no_host_ambient_authority` source-grep gate (`lib.rs:87`) iterates every `.rs` in
`src/`, so `net.rs` is automatically in scope with no test edit required — and it passes.
The module deliberately uses no host APIs; the loopback wiring is correctly delegated to
run control ("the loopback wiring … is RUN CONTROL's, not this device's", `net.rs:4-5`),
keeping the device on the pure-replay side of the §6 deny-list boundary. The words "net",
"network", and "loopback" appear in prose but none collide with the forbidden tokens
(`std::net`, `rand::`, etc.), so the gate stays green honestly.

### P-6. Correct, anticipated `DEVICE_ID` / tag wiring

`DEVICE_ID_PV_NET = 0x0007` matches the value `dhsnap.rs:93` already anticipated
(`0x0007 => Some(tag::NETL)`), and the dedicated test `device_id_is_the_dhsnap_pinned_0x0007`
pins it so the two cannot drift. This is exactly the failure mode the `mmv` bead notes
warned about ("MUST define DEVICE_ID_PV_NET = 0x0007 … or the map silently diverges") — and
it has been closed with an assertion.
