# Critical and Important

## CRITICAL-1 — The §7.2 masked CPUID table is NOT deterministic; it leaks host-CPU identity, and the committed artifact's hash does not reproduce

`crates/dh-vmm/src/cpuid.rs` line 1 promises "the guest sees one fixed, hashed CPUID table," and `cpuid_table_hash` "feeds the MachineConfig determinism class." Both claims are false on this lab box.

### Observed (live, same binary, 6 runs)

`./target/debug/dh-cli cpuid-diff | tail -1` flips between two hashes:

```
4dac1b7a9ba1ddb08da173ab14e3077a91b0e9024ecaf4ba9b5d05c1f46bdc03   (committed value — 1 of 6 runs)
65be80759e6ff65db310c595041da2c9c8a15522d802a5ec909f18c841712d38   (5 of 6 runs)
```

The committed `docs/ops/cpuid-diff-infra-control.txt` therefore does NOT reproduce:
`cargo run -p dh-cli -- cpuid-diff | diff - docs/ops/cpuid-diff-infra-control.txt` fails on line 11 (the hash) most of the time. The 10 *diff* lines (the masked-changed dwords) ARE byte-stable, which is exactly why the artifact "looks" reproducible to a casual reviewer.

### Root cause (dumped the full sorted masked table across runs)

Two CPUID fields the §7.2 mask does NOT touch carry host-CPU-placement state and vary run-to-run as `KVM_GET_SUPPORTED_CPUID` runs on whatever logical CPU the ioctl landed on:

- **Leaf 0x00000001 EBX**: `0x02100800` vs `0x00100800` — delta `0x02000000` is bits [31:24], the **Initial APIC ID**.
- **Leaf 0x0000000B subleaf 0 EDX**: `0x00000002` vs `0x00000000` — leaf 0xB EDX is the **x2APIC ID of the current logical processor**.

Both pass through `mask_in_place` untouched (they hit the `_ => {}` arm, cpuid.rs ~line 110). They never appear in the printed diff because masking doesn't change them — so the artifact hides the very fields that make it non-reproducible.

### Why this is Critical, not cosmetic

1. The masked table is fed verbatim to the guest via `KVM_SET_CPUID2` (ARCH §7.2 / line 106). A guest that reads leaf-1 EBX[31:24] or leaf-0xB EDX (any topology-aware boot/SMP-probe path, libc, runtime) observes a value that differs between the recording host CPU and any replay host CPU — and even between two runs pinned to the same nominal core if the ioctl floats. That is a direct **replay-divergence vector**, precisely the §7 class this mask exists to close (it already removes x2APIC and kvmclock for the same reason).
2. `cpuid_table_hash` is part of the `MachineConfig` determinism tuple. A non-deterministic determinism-class hash means two recordings of the same VM on the same host can disagree on their machine identity — silently.
3. Committing a host-specific snapshot whose hash is unstable on its own host defeats the artifact's stated purpose ("infra control"). The filename label does not save it: the value it asserts is wrong 5/6 of the time.

### Fix

Extend the §7.2 mask in `mask_in_place` to pin the host-placement fields to fixed values (single-vCPU, no-APIC contract makes this trivial):
- Leaf 1 EBX: clear/zero bits [31:24] (initial APIC ID) — arguably also bits [23:16] (max addressable IDs), which for a 1-vCPU machine should be a constant.
- Leaf 0x0000000B and 0x0000001F (extended topology v1/v2): EDX (x2APIC ID) to 0, and ideally zero the whole leaf since x2APIC is already cleared in leaf 1 ECX and there is "no APIC at all in the direct-vector contract."

Then regenerate `docs/ops/cpuid-diff-infra-control.txt` and add a unit test asserting `cpuid_table_hash(masked)` is invariant under a re-fetch (the existing `assert_eq!(hash(masked), hash(masked))` reuses the SAME fetch, so it cannot catch this — it must compare two independent `masked_cpuid(&kvm)` calls).

> Note: `cpuid_table_hash` itself sorts by `(function,index)` correctly. The bug is upstream content (unmasked host-variable fields), not the hash framing. The diff CLI's `BTreeMap` also silently collapses any duplicate `(function,index)` entries, so if KVM ever returns flag-distinguished duplicates the diff and the hash would disagree on cardinality — worth a guard, but the topology leaves are the live failure here.

---

## IMPORTANT-1 — `state_hash` in the run-twice compare does NOT include device sections

`run_segment` calls `chain.push_final_link(seg.slot, &[], …)` (runctl.rs:318, :405, etc.) with an **empty** `device_sections` slice. `hash.rs` has a perfectly good `device_sections(bus)` harvester (id|version|len|bytes per device in base order) and `push_final_link` accepts the argument — but the M1 path can't reach the bus (devices live inside the `on_exit` closure, not in `Segment`), so the compared fingerprint covers **vCPU + full guest RAM only**.

Consequence for this acceptance: the run-twice `state_hash` equality does NOT directly prove device-internal state (blk overlay dirty clusters, entropy stream `word_pos`, channel producer seqs, pad latch/frame counter) is identical between runs. The test compensates indirectly and the coverage is real but partial:
- blk overlay correctness → the guest's 'B' read-back compare lands in guest RAM (hashed). OK for THIS guest.
- entropy → `record_count` (the ENTROPY digest) + the guest's not-all-zero check in RAM (hashed).
- channel → `record_count` + `beacons`.

So a device-state divergence that does NOT reflect into guest RAM or the record stream (e.g. an entropy `word_pos` that lands on the same 32 output bytes, or a channel producer-seq drift) would pass the run-twice compare undetected. This is not wrong for M1, but the test's headline claim — "the whole run is bit-identically repeatable" — overstates what `state_hash` proves. Either fold `device_sections(&bus)` into a final fingerprint after the run (compare `out.device_sections` between runs), or downgrade the comment to scope it to vCPU+RAM. The product needs the former eventually (M3 replay compares device sections); doing it here closes the gap cheaply since the test already owns `bus` after `run_segment` returns.

---

## IMPORTANT-2 — Attach success is proven only transitively; assert it directly

Interrogation concern #1: the guest branches on `IN 0xD37C` status == 0 and on `IN 0xD380` == 0; if BOTH runs read identical garbage, determinism would hide a fake "success." In practice attach IS real and the test IS safe, but only transitively:

- `pio_in(PORT_INIT_GO)` returns `self.init_status`, which is set by `channel_init` (detchannel.rs:330). A failed attach yields a nonzero `InitStatus` → guest emits lowercase 'd' → `assert_eq!(serial, "CEPBDX")` fails. So a fake-zero status cannot pass.
- The doorbell `drain` early-returns `Vec::new()` when `self.channel` is `None` (detchannel.rs:454). The test asserts `out.beacons.len() == 1` with the exact `DEVICE_EXERCISE_BEACON_ID`, which is only reachable if `Channel::attach` succeeded and a real ring-W record decoded. So the Beacon assertion transitively proves attach.

This is sound, but it is two hops of reasoning. Add one belt-and-suspenders line so a future refactor of either path can't quietly regress to "garbage that happens to be 0": expose and assert `channel_r.channel().is_some()` (the getter already exists, detchannel.rs:278) after the run, or assert `host.metrics.drain_failures == 0` and `host.init_status == InitStatus::Ok as u32`. Cheap, and it makes the "status 0 is REAL" claim explicit rather than emergent.
