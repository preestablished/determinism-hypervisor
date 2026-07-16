# Positive notes

## The `F_SEAL_WRITE`-avoidance reasoning is correct *and* documented — and I verified it

`crates/dh-vmm/src/kvm.rs:243-247` explains that `F_SEAL_WRITE` is deliberately omitted
because the kernel refuses it while a live writable mapping (the parent's KVM
guest-RAM mapping) exists, so `F_SEAL_FUTURE_WRITE` + the software `Frozen` guard split
the job. This is exactly right — I confirmed on 6.8 that a new `MAP_SHARED|PROT_WRITE`
mapping post-seal returns EPERM while the existing parent mapping keeps writing (the test
`write_slice(... GuestAddress(0x2000))` at kvm.rs:601 proves the latter live). It's rare
to see the *negative* design decision (why NOT the stronger seal) written down with the
kernel mechanism that forces it. This will save the fork-bead author a day.

## The transition matrix test is the right shape — exhaustive, not anecdotal

`slot_state_tests::transition_matrix_is_exactly_the_spec_relation` (lib.rs:198-223)
double-loops over `ALL × ALL` and checks `can_transition` against an explicit allow-list,
*and* round-trips `transition()` to confirm the `Ok`/`Err` arms agree. That's the correct
way to test a small state machine — it can't drift out of sync with `can_transition`
because both are checked against the same source-of-truth list, and any future edge
added to `can_transition` without updating the list fails loudly. `no_self_transitions`
(lib.rs:226-230) is a nice belt-and-suspenders invariant.

## The live freeze test asserts the *whole* contract, including the negatives

`freeze_ram_seals_future_writes_but_not_the_live_mapping` doesn't just check
"FUTURE_WRITE is set." It asserts (a) pre-freeze absence, (b) post-freeze presence of all
three seals, (c) `F_SEAL_SEAL` *absence* (idempotence precondition), (d) re-freeze is a
no-op, (e) new RW mmap → EPERM, (f) new RO mmap → OK, (g) existing mapping still writable,
(h) truncate → error. That's a precise encoding of the R9 hardware contract; the EPERM
errno check (kvm.rs:586) in particular pins the *reason* for the failure, not just that
it failed. Excellent.

## `ensure_write_path` carries the caller name into the error

`FrozenWriteDenied { api: &'static str }` (lib.rs:50) means when this guard fires in
production it'll say *which* write-path call was attempted on a frozen slot — turning a
"corrupted CoW child" mystery into a one-line "X called inject_inputs on a Frozen slot."
For the single software guard standing between a stray call and cross-branch
contamination (R9), that observability is well-placed.

## Comments tie code to spec sections precisely

Both new blocks cite `ARCH §8.4`, `§2.2`, and `risk R9` inline (kvm.rs:233-247,
lib.rs:42-47, 63-77). I cross-checked each against `.agents/docs/.../ARCHITECTURE.md` and
`API.md` and they're accurate (the only stale text is on the *doc* side — see 01 I-2, not
the code's fault). This is the kind of bidirectional traceability that makes the fork epic
auditable.

## `MFD_ALLOW_SEALING` + no premature `F_SEAL_SEAL` keeps the door open correctly

By not adding `F_SEAL_SEAL`, the design leaves room for the fork path to add further seals
later (or re-apply) without the parent locking itself out, while still getting full
idempotence. Subtle and correct.
