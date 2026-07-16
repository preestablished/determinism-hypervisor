# Positive Notes

### P1. The teardown ordering is correct and race-free, for a non-obvious reason

`drop(store1)` → `handle1.shutdown()` → `drop(rt1)` (lines 176-178) is the right
order, and crucially it is race-free. The blocking client owns its **own**
current-thread runtime (`blocking.rs:27`) and `block_on`s each RPC, so by the
time `take_snapshot` returned the delta ref the server's `put_snapshot` —
including its `pages.sync()` barrier and manifest fsync+rename — had already
completed. The test is single-threaded and synchronous, so there are no in-flight
puts for `handle1.shutdown()` (which only *signals*, never drains) to drop on the
floor, and `drop(rt1)` tearing down the server runtime cannot lose committed
work. Dropping the client first also closes the UDS connection cleanly before the
server task is signalled. This is exactly the ordering a reviewer would worry
about, and it holds.

### P2. The `sock_name` seam avoids the stale-socket-unlink trap entirely

Giving each instance a distinct UDS (`first.sock` / `second.sock`) instead of
relying on instance 1's socket file being unlinked is the right call. The server
*does* remove a stale socket on bind (`build_server.rs:275-277`), but depending
on that across a teardown is a latent race; distinct names sidestep it. The
docstring at `common/mod.rs:45-48` explains precisely this reasoning. Clean seam:
`spawn_store_blocking` now delegates to `spawn_store_at`, so the existing three
test targets are unchanged behaviorally and the `TempDir` ownership split
(caller-owned root for the durability test, helper-owned for the rest) is
correct.

### P3. The guest-dirtied delta is genuinely exercised through KVM, not faked

The delta pages (`0x2000`/`0x5000`/`0x9000`) are written by *guest* `mov`
instructions executed under `vcpu.run()` with dirty-ring harvesting
(lines 96-124), so the incremental `PageSource` is driven by the same dirty-set
machinery production uses — the ring only sees guest writes. This makes the
"delta survives restart" claim meaningful: a fabricated dirty set could mask a
flatten bug, but a real guest-dirtied set cannot.

### P4. Correctly scoped away from the meta DB

The engine restore path uses only `get_snapshot` + `resolve_pages` (verified in
`restore_engine.rs:128,157`), never `create_node`/`query_nodes`. So the startup
reconcile step that can mark meta nodes `PRUNED` when a ref does not resolve
(`startup.rs:209-273`) is entirely out of this test's path — there is no hidden
way for a half-reconciled meta tree to make the test pass or fail spuriously. The
durability surface under test is exactly the manifest `.spm` + pack `.spk` layout,
which is what R12 is about.

### P5. Honest, well-cited scope framing in the module doc

The doc header (lines 1-17) correctly enumerates which legs already live in
sibling tests (round-trip fidelity, parent-relative deltas, ref-after-ack) and
names the precise gap this file fills. It cites `store_joint.rs` for the R12 wire
pins and is explicit that every sibling runs against the real in-process server,
never a mock. The bead-scope honesty the prompt asked about checks out: the bead
defers "server-on-runner mechanics" to the CI fixture bead and "ref returned only
after store durability ack" is exactly what the positive path + the pins pin —
this file is the durability-of-the-receipt complement, correctly placed with the
engines (per ARCH §1's "nothing depends on dh-worker" rule, as `m4_transparency`
already documents).

### P6. Assertion messages are specific and failure-localizing

Each assert carries a message that names what broke and where:
`"delta page at {gpa:#x} after restart"`, `"root-era page after restart"`,
`"vCPU state after restart"`, `"restart changed the bytes a restore reproduces"`.
A failure in CI would immediately tell you whether it was a delta page, a
root-era page, the vCPU blob, or the ref — no debugger round-trip needed.
