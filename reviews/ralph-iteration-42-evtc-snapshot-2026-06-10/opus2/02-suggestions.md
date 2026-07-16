# Suggestions

### S-1 — Add a `const` assertion that `EVTC_LEN` equals the bytes `snapshot()` actually writes

`EVTC_LEN = 4 + 4 + 4 + 5 + 5 + 1 + 16 = 39` is correct (verified by summing the writer's pushes: three u32s = 12, two `u8 flag + u32` = 10, then `1 + 16` for the channel flag + 8-byte gpa + two u32 seqs = 17; 12+10+17 = 39). But the constant is hand-maintained and decoupled from `snapshot()`. A future edit to the layout could desync them, and `restore()` validates against `EVTC_LEN` while real captures come from `snapshot()` — a silent divergence would pass the roundtrip test (both wrong together) only if the test reuses the same constant, which it does. Add a unit test that asserts `let mut v = Vec::new(); host.snapshot(&mut v); assert_eq!(v.len(), EVTC_LEN)` for *both* the attached and detached cases. The attached case is already covered in `evtc_roundtrips_attached_state_and_seqs`; add the detached-case length assertion to `evtc_roundtrips_detached_state_and_refuses_bad_input` (it currently checks behavior but not the byte length of the detached section).

### S-2 — Add a byte-identical-roundtrip determinism test, matching the blk precedent

`blk.rs` has `restore_then_snapshot_is_byte_identical_and_keeps_host_io_errors`. EVTC has roundtrip-by-behavior tests but no `snapshot -> restore -> snapshot` byte-equality test. Since the EVTC section is all scalars (no `HashMap`, no ordering concerns — determinism is structurally guaranteed), this test is cheap and would lock in that the section is canonical. Recommend adding it to mirror the sibling device's coverage and to guard against a future field that *does* introduce nondeterminism.

### S-3 — Out-of-bounds GPA restore is covered, but only implicitly

The prompt's "flag=1 but gpa points OUTSIDE mock memory" scratch case is in fact covered by the *same* refusal path as the bad-header test: `Channel::attach(self.mem.clone(), gpa)` returns `Err` (header read fails / out-of-range), `.map_err(|_| RestoreError)` converts it, and `restore()` refuses. The existing `bad header at GPA refuses` test exercises a valid-range-but-zeroed GPA. Consider adding an explicit OOB-GPA variant (e.g. `bad[23..31].copy_from_slice(&0xDEAD_0000u64.to_le_bytes())`) so the "attach Mem error -> refusal" branch is named and won't silently regress if attach's error taxonomy changes. Low priority — same code path, just better-documented coverage.

### S-4 — Note the post-restore degraded inject-name-resolution window in the fork doc too

The `snapshot()` doc correctly states the intern/pending-inject caches are not serialized and that "post-restore inject-point name resolution degrades to None, which FaultPlans must tolerate" until the orchestrator replays the drained event stream. Good. For the §8.4 fork child this window also exists (the child starts with empty intern caches). When you add the I-2 fork breadcrumb, mention that the child inherits this degraded-resolution window until its fresh DHILOG/replay re-seeds the caches — so the fork bead's author wires the cache-replay step, not just the seq restore.
