# Positive Notes

1. **The durability claim is backed by a real fsync path, not assumed.** Against an in-process restart it would be easy to ship a test that "passes" purely because the OS page cache survived. It doesn't here: `put_snapshot`'s group-commit `gc.barrier(|| self.pages.sync())` fdatasyncs every dirty pack and the manifest before the ref returns. The test rides the genuine durability contract. (Verified in `snapstore-store/src/lib.rs:380-437`, `snapstore-pagestore/src/ingest.rs:652-680`.)

2. **It does not lean on graceful shutdown.** Because durability is established at ack, not at shutdown, dropping the client + calling `handle1.shutdown()` (a fire-and-forget oneshot, no flush, no wait) + dropping the runtime tears down *all* of instance 1's in-process state, leaving only on-disk bytes. The test is closer to a hard kill than a clean stop — the strong form of the property R12 cares about.

3. **The `spawn_store_at` seam is the minimal, correct refactor.** Caller-owned `data_root` + per-instance `sock_name` is exactly what restart-over-the-same-bytes needs, and `spawn_store_blocking` is preserved by wrapping it — zero churn for the four existing test targets. No copy-paste of the readiness-probe rig.

4. **Per-instance UDS names avoid a real flakiness vector.** `first.sock` / `second.sock` (rather than reusing one socket file) sidesteps any dependency on the first instance's socket being unlinked before the second binds. `serve_for_tests` does `remove_file` stale sockets defensively, but not relying on that is cleaner.

5. **The captured-source comparison is set up correctly.** `slot_a` is deliberately kept alive to the end so `vcpu_state::capture(&slot_b2) == vcpu_state::capture(&slot_a)` compares the restored machine against the *live* source slot, not a snapshot of it — the right ground truth, and it forces the vCPU state through the persisted-and-reloaded path.

6. **The strongest assertion is present.** Beyond byte-identical RAM + vCPU state, the re-snapshot-yields-identical-ref check (`ref_b2 == ref_b1`) proves content-addressed identity is carried entirely by the persisted bytes — a single 32-byte equality that catches any subtle divergence the byte-by-byte checks might miss. This is the receipt-is-durable claim stated in its tightest form.
