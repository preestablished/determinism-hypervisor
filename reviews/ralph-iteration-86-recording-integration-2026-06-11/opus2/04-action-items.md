# Action Items — Iteration 86 (bead y78), 2nd reviewer

Self-contained; each item names the file, the problem, and the fix.

---

### Critical

- [ ] **Fix `drain_net_tx` to gate on `STATUS_OK` and cap `len` — it mints loopback frames for faulted TXs.**
  `crates/dh-vmm/src/recording.rs:248-263`. `drain_net_tx` reads `net.tx_regs()` (last-*programmed* `tx_buf_gpa`/`tx_len`), which is **uncapped** and **does not reflect `tx_status`**. The device's `doorbell` (`crates/dh-devices/src/net.rs:100-113`) sets `STATUS_FAULT` and logs **no NET_TX record** when `tx_len > MAX_FRAME` or the read faults. So: guest programs `tx_len = 4000` (> `MAX_FRAME = 2048`), rings doorbell → FAULT + no NET_TX → run control calls `drain_net_tx` → it reads `len = 4000`, does `vec![0u8; 4000]`, `mem.read` succeeds, returns a frame → loopback lands a NET_RX with **no antecedent NET_TX record** = replay divergence. Plus `vec![0u8; len as usize]` is a guest-controlled `u32` allocation up to **4 GiB** (the doorbell caps before allocating; `drain_net_tx` does not).
  **Fix:** add a `PvNet::tx_status()` (or return status from `tx_regs`) and gate: `if status != STATUS_OK || len == 0 { return Ok(None); }`. `STATUS_OK` implies `len <= MAX_FRAME` (the doorbell guarantees it), eliminating the unbounded alloc. Optionally upgrade the post-`STATUS_OK` `mem.read` failure from `Ok(None)` to a loud error (a frame that read cleanly at the doorbell should read cleanly at the same exit).
  **Note:** latent — no live caller yet (loopback is bead **czq**, OPEN), so it does NOT block y78's merge. But it is a P0 divergence the instant czq wires the drain. File as P0 and land before/with czq.

---

### Important

- [ ] **File a P0 bead for the missing EPOCH_HASH writer — a5e is blocked without it.**
  No `LogWriter::epoch_hash()` method exists; grep across `crates/` finds only the kind constant (`dh-inputlog/src/dhilog.rs:48`), the flag (`dhilog.rs:39`), and reader/consumer code (`reader.rs`, beads 1py/a5e/pee). **Nothing emits EPOCH_HASH records into a DHILOG during a recording run.** a5e ("every EPOCH_HASH equal") and 1py ("checks EPOCH_HASH records against the live chain") both consume records that no producer creates.
  **Fix:** file a P0 bead: *"DHILOG EPOCH_HASH writer: add `LogWriter::epoch_hash(icount, epoch_hash)` and a run-loop cadence that logs the live `StateHashChain` epoch hash at each epoch boundary (set `FLAG_EPOCH_HASHES`)."* Make `a5e` depend on it (alongside 39w). Out of y78's scope (y78 = PAD_SET/DEV_EVENT/NET_RX + AUX), so it does not block y78 — but it is the next gap on the a5e critical path and currently owned by no bead.

- [ ] **Fix the module doc's "segment" terminology drift — doc says "build fresh per segment", test (correctly) reuses one rail across 3 `run_segment` calls.**
  `crates/dh-vmm/src/recording.rs:64-67`. The `DeviceRail` doc says "Build it fresh per segment" but `pad_echo_live_run_records_inputs_frame_marks_and_seals` builds one rail / one `LogWriter` across three `run_segment` quanta and seals once. The **test is correct for M5**: a DHILOG covers a **recording span (snapshot-to-snapshot)** spanning **many `run_segment` calls**, not one log per `run_segment`. The doc conflates the two meanings of "segment".
  **Fix (doc-only):** define "segment" once at the module top as the snapshot-to-snapshot recording span; change "build fresh per segment" to "build one rail per recording span (snapshot-to-snapshot), spanning many `run_segment` calls; seal once at span end." The `seal(self, ...)` signature is already right.

---

### Suggestions

- [ ] **Document the `boundary_rip` asymmetry.** `recording.rs:147-148` hard-codes `boundary_rip = 0` in `service_exit` (vCPU regs unretrievable mid-segment); `apply_*` take the real rip from the outcome (applied between segments, regs available). Correct and intentional — add one doc line on `apply_pad_set` so nobody "fixes" the 0 to match.

- [ ] **Add a test for the vector *delivery* leg (M5 interrupt path).** `recording.rs` queues edge vectors into `self.irqs`; host test proves queueing + seal guard, but nothing proves `irqs → ScheduledInjection → next-segment delivery → guest ISR`. Out of y78's scope (run-control conversion); file a follow-up: flip RX_VECTOR/IRQ_VECTOR on, apply, drain into `ScheduledInjection`, assert the guest ISR fired.

- [ ] **Rename the seal guard's error variant.** `recording.rs:289-291` returns `RecordError::NoDevice("undrained irq queue at seal")` — semantically wrong (`NoDevice` = missing/non-downcasting device). Add `RecordError::UndrainedInjections` so a dropped-injection seal failure is distinguishable from a config error downstream.

- [ ] **(Optional) Drop the redundant inner `target_arch = "x86_64"` on `mod live_tests`** for consistency with `mod tests` (`#[cfg(test)]`). The whole module is x86-gated at `lib.rs:32-33`, so the inner predicate is always true — redundant but harmless. Adds no behavior; only removes the false impression that `mod tests` builds on non-x86.

- [ ] **(Optional) De-duplicate `device_mut` vs `apply_net_rx`'s open-coded split-borrow loop** (`recording.rs:163-178` vs `200-220`) if a third such call appears. The duplication is currently justified (mem+device simultaneous borrow) and well-commented.
