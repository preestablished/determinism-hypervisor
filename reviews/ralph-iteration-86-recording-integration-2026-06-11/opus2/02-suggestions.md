# Suggestions

---

## S1: Document the `boundary_rip` asymmetry between `service_exit` (0) and `apply_*` (real rip)

**File:** `recording.rs:147-148` (service_exit `boundary_rip = 0`) vs `apply_pad_set`/`apply_net_rx` (caller passes `outcome.boundary.rip`).

`service_exit` hard-codes `boundary_rip = 0` with a good inline rationale ("not retrievable while the segment holds the vCPU"). The `apply_*` methods instead take a real `boundary_rip` and the live test passes `o1.boundary.rip` (the genuine boundary rip from the finished segment outcome). **This asymmetry is correct and intentional** — `apply_*` runs *between* segments when the vCPU regs ARE retrievable, while `service_exit` runs *during* a segment when they are not. But the asymmetry is non-obvious and a future reader could "fix" the `0` to match. Add one line to the `apply_pad_set` doc noting that, unlike `service_exit`'s 0-rip debug-loop convention, canonical inputs land at the *real* boundary rip from the outcome because they apply between segments. (The records must land at the canonical boundary icount — verified: the live test passes `o1.boundary.icount`, which IS the canonical boundary — good.)

---

## S2: Vector-injection leg (IRQ_VECTOR → apply → queued → ScheduledInjection → delivered) has no end-to-end test

**Files:** `recording.rs` apply_* push to `self.irqs`; `runctl.rs:101-144` (`ScheduledInjection`/injection types).

Both `apply_pad_set` and `apply_net_rx` push the returned edge vector onto `self.irqs`, and `seal` refuses an undrained queue. The **host test** exercises the queue (sets RX_VECTOR=0x41, asserts `irqs.len()==1`, drains, then seals) — good, that proves the *queueing* + the *seal guard*. But nothing proves the **delivery leg**: rail.irqs → run control converts to `ScheduledInjection` → next segment's `injections: &[]` actually delivers it → guest ISR runs. The live test deliberately leaves the pad vector disabled (`assert!(rail.irqs.is_empty(), "pad vector disabled")`), so the whole interrupt path is untested end-to-end.

This is **out of y78's strict scope** (y78 = "inputs + AUX at the correct boundary icount"; the bead doesn't mention interrupt delivery), and the conversion from `irqs` to `injections` is run-control's job, not the rail's. But it IS the M5 interrupt leg and someone should own a test that flips RX_VECTOR/IRQ_VECTOR on, applies, drains into `ScheduledInjection`, and asserts the guest ISR fired. Recommend filing it as a follow-up rather than blocking y78. Note: research file flags "asserting only the happy path; missing the error/boundary variants" — the queue's *delivery* boundary is exactly that missing variant.

---

## S3: The inner `#[cfg(all(test, target_arch = "x86_64"))]` on `mod live_tests` is redundant (harmless)

**File:** `recording.rs` (`mod live_tests` cfg) vs `lib.rs:32-33` (`#[cfg(target_arch = "x86_64")] pub mod recording;`).

`recording.rs` is only compiled on x86_64 (gated at `lib.rs`), so inside it the `target_arch = "x86_64"` predicate is always true. `mod tests` uses plain `#[cfg(test)]` while `mod live_tests` uses `#[cfg(all(test, target_arch = "x86_64"))]`. The extra predicate is **redundant but harmless** and arguably documents intent (these need real KVM). The asymmetry between the two test mods could read as "tests runs on non-x86" — it doesn't; both only ever build on x86 because the whole module is x86-gated. Optional: drop the inner `target_arch` for consistency, or add a one-line comment that it's belt-and-suspenders. Not worth churn on its own.

---

## S4: `device_mut` is O(devices) per call and `apply_net_rx` duplicates its body

**File:** `recording.rs:163-178` (`device_mut`) and `recording.rs:200-220` (`apply_net_rx`'s open-coded split-borrow loop).

`apply_net_rx` can't use `device_mut` because it needs `&mut self.mem` *and* `&mut net` simultaneously (the split-borrow dance, well-commented). That's a legitimate reason to open-code, but the loop body is a near-duplicate of `device_mut`. If a third such call appears, consider a small helper that returns the device index/`&mut dyn` plus splits mem out, or restructure `device_mut` to return an index so callers can re-borrow. Minor; the comment already explains the duplication. The linear scan over `devices_mut()` is fine at current device counts.

---

## S5: `RecordError::NoDevice("undrained irq queue at seal")` overloads a misnamed variant

**File:** `recording.rs:289-291` (`seal`).

`seal` returns `RecordError::NoDevice("undrained irq queue at seal")` when `self.irqs` is non-empty. `NoDevice`'s doc says it means "the bus has no device with the id" or "does not downcast" — an undrained IRQ queue is neither. Reusing the variant works (the `&'static str` carries the real meaning) but it's semantically wrong and would confuse error-classification downstream (e.g., a slot-fault categorizer that treats `NoDevice` as a config error vs. a drop-injection bug). Add a dedicated variant, e.g. `RecordError::UndrainedInjections`, so the seal guard's failure is distinguishable from a missing-device failure. The guard itself is correct and valuable.
