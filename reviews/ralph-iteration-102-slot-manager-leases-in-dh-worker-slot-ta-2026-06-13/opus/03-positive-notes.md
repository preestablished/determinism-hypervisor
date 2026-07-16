# Positive Notes

### P1 — State machine has exactly one home, and the manager actually defers to it

The headline claim ("all transitions delegate to `can_transition`") survives a
line-by-line audit. Every landed state comes from `transition()` except three
direct writes, each provably guarded (`release` Frozen→Paused under a
`== Frozen` check; `force_destroy` child Faulted under an explicit
`can_transition(Faulted)` check; `reclaim_expired` Running→Faulted, a legal edge,
inside its own match arm). No write-path entry point skips `ensure_write_path`.
For a security-adjacent control module this is the property that matters most, and
it is genuinely upheld.

### P2 — Lease validation ordering is correct *and* quietly security-aware

`validate_entry` checks bounds (NoSuchSlot) → token equality → expiry. The token
match precedes the expiry check, so a caller without the right token always gets
`StaleLease` and never learns whether the lease had expired. That is the correct
precedence per INTEGRATION §2 *and* avoids leaking lease-state information to a
non-holder. The distinct `StaleLease` vs `LeaseExpired` variants (both
FAILED_PRECONDITION on the wire, but separable for diagnostics) let reclaim races
be diagnosed without weakening the wire contract.

### P3 — Faults are deliberately *not* lease-gated, with a sound justification

`mark_faulted` takes a bare `slot_id`, no lease. The reasoning (faults are the
worker's own observations — divergence, DATA_LOSS, counter revocation — and must
land even when the orchestrator's lease has expired mid-run) is exactly right: a
lease-gated fault path could strand a diverged slot as "healthy" after lease
lapse. The `reclaim_expired` Running→Faulted handoff (keep the expired lease in
place so the *next* sweep frees the now-Faulted slot, rather than clearing it into
a state only `force_destroy` could reach) is a genuinely subtle correctness call
that the code gets right and the test pins.

### P4 — All-or-nothing fork is real, not aspirational

`fork` does the transition check and the free-slot count check *before* mutating
anything, and only commits the parent freeze + child registration once both pass.
`fork_is_all_or_nothing_when_slots_run_out` proves the parent stays Paused with
zero children and the lone Empty slot survives a failed 2-child fork. This is the
correct shape for a resource-exhaustion path and avoids orphaned child leases.

### P5 — The syscall/unsafe split honors dh-worker's no-unsafe rule honestly

Putting `pin_current_thread` / `set_current_thread_fifo` in `dh-vmm::run` (which
permits scoped `#[allow(unsafe_code)]`) rather than smuggling unsafe into
dh-worker is the right architectural call, and the SAFETY comments are accurate
(`cpu_set_t`/`sched_param` are plain data; `0` targets the calling thread). The
affinity/scheduler split into separate `PinError` variants lets the daemon treat a
FIFO EPERM as non-fatal on dev boxes while a real affinity failure stays fatal —
the live test exercises exactly that EPERM-tolerance path.

### P6 — The deny-grep is robust on the exact point flagged

`path.ends_with("proto_map.rs")` uses `Path::ends_with`, which matches whole path
*components*, not substrings (empirically verified: `map.rs` does not match,
`proto_map.rs` matches only the full filename). So the exemption is precise — it
won't accidentally exempt a `not_proto_map.rs`, and it won't fail to exempt the
real file. Pairing the i32-cast ban with the proto-map unit tests that pin each
enum arm to its wire number is a strong belt-and-suspenders against the
domain-vs-proto discriminant mismatch (`Running` = 1 domain vs `RUNNING` = 3 wire).

### P7 — Time is kept out of the manager on purpose

`now_ms` is caller-passed everywhere; the manager never reads a clock. This keeps
host wall-clock out of guest-visible state (the project's central determinism
discipline) *and* makes every expiry/reclaim test deterministic — the TTL tests
read cleanly because of it. Small decision, large payoff.
