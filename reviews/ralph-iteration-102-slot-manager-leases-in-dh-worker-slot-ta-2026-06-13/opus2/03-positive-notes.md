# Positive Notes

These are the things this change got right that I specifically went looking to
break and could not.

### P1 — `fork` is genuinely all-or-nothing on every `?` path

The ordering is deliberate and correct: `validate_entry` → `ram_is_cow` refusal →
`transition(Frozen)` computed into a **local** `frozen` (not yet stored) →
free-slot collection → `free.len() < children` bail → *only then* commit
`slots[parent_idx].state = frozen` and the child mutations. Every early return
before the commit point (`NoSuchSlot`, stale lease, `ram_is_cow`, illegal
transition, `NoFreeSlot`) leaves the table untouched. The
`fork_is_all_or_nothing_when_slots_run_out` test pins the parent staying Paused
and zero orphan leases. This is the part I expected to find a half-committed
mutation in, and it holds.

### P2 — The cross-tenant force-destroy cascade clears `parent` links correctly

`force_destroy` faults every live child **and** sets `parent = None` before
releasing the parent slot for reuse. That single line is what makes the
"wrong-tenant auto-thaw" attack impossible — an orphaned child destroyed later
sees `parent = None` and never reaches into the reused slot. The
`force_destroy_cascades_faults_to_cow_children` test covers the cascade; (only
the *reuse-then-orphan-destroy* ordering is untested — see S1).

### P3 — Time is injected, never read

`now_ms` is a caller parameter on every time-sensitive entry point and the module
holds no clock. This keeps host wall-clock out of guest-visible state (the
determinism invariant) *and* makes every expiry/renew/reclaim test fully
deterministic with literal timestamps. The TTL tests read cleanly because of it.

### P4 — Single source of truth for the state machine, consistently delegated

Every transition goes through `SlotState::can_transition` / `transition`, and
every write-path entry composes `ensure_write_path`. The manager adds zero
parallel transition logic of its own — exactly the "exactly one home" the module
doc claims. This is the right adoption of the R9 guard that dh-vmm's lib.rs
flagged as "INTEGRATION (not yet wired)."

### P5 — The deny-grep `as i32` test is correct and well-targeted

`Path::ends_with("proto_map.rs")` matches whole path components, so it correctly
exempts exactly the bridge module and nothing else; `path.extension() != "rs"`
skips non-source. Pairing the runtime grep with the per-arm wire-number unit test
(`Frozen → 4`) means both a stray domain cast *and* a proto renumber break
loudly. This is a strong, low-cost guard against the precise class of bug
(`SlotState::Running as i32 == 1` vs proto `RUNNING == 3`) the offset/order
mismatch invites.

### P6 — Lease replay across slot reuse is closed at the source

`release()` → `SlotEntry::empty()` drops the old `(token, expires)`, and the next
`allocate`/`fork` mints a fresh token. A stale `Lease` from a previous tenancy
fails `validate_entry` because the stored token no longer matches. The only
theoretical replay requires `/dev/urandom` to emit the same 16 bytes twice — not
a practical concern (noted as a non-issue in 02, not a finding).

### P7 — `mark_faulted` deliberately ungated, and the reasoning is sound

Faults are the worker's own observations and must land even when the
orchestrator's lease has expired mid-run; gating them on a lease would let an
expired lease hide a divergence. The asymmetry (faults ungated, everything else
gated) is the correct call and is documented at the function.

### P8 — `pin_current_thread` SAFETY reasoning is accurate

`mem::zeroed::<cpu_set_t>()` is equivalent to `CPU_ZERO` (the set is a plain
bitmask array with no header), `sched_setaffinity(0, ..)` targets the calling
thread, and the affinity vs SCHED_FIFO split correctly reflects that only the
FIFO promotion needs CAP_SYS_NICE. The live test reads the affinity back with
`sched_getaffinity` and asserts `CPU_COUNT == 1` — a real confinement check, not
just "syscall returned 0" — and tolerates exactly `EPERM` on the FIFO step.
