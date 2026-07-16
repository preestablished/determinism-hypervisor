# Suggestions (non-blocking)

## S1. The `Frozen → Faulted` exclusion rationale is sound, but slightly overstated

The doc comment justifies omitting `Frozen → Faulted` with: "a frozen parent
executes nothing and accepts no writes, so nothing can fault it." As a
statement about *guest-contract / determinism* faults this is airtight — a
frozen parent runs no instructions, so it cannot diverge, overshoot a boundary,
or violate the guest contract. The CHILD-faults-are-the-child's-state point is
also correct: a faulting CoW child transitions *its own* slot to `Faulted`; the
parent is untouched.

**Devil's advocate (HOST-side faults):** a frozen parent could still suffer a
*host-side* failure that is discovered late — e.g. a memfd truncation attempt,
an `F_SEAL_WRITE`/seal-verification failure surfaced only when a new child maps
the baseline, or a `ram_seals` read error. Those are not guest-contract faults,
but they *are* "this slot's state is no longer trustworthy" events, which is the
literal definition the `Faulted` doc gives. So the comment's "nothing can fault
it" is true for the *guest-contract* class but slightly too strong as written
for the *host-integrity* class.

**However, the conclusion is still right:** the correct response to a host-side
seal/memfd integrity failure on a frozen parent is **Destroy**, and
`Frozen → Empty` already exists. Routing a host-integrity failure through a
`Frozen → Faulted` edge would buy nothing — `Faulted`'s only exit is `Empty`
anyway, so it would just be a longer path to the same DestroyVm. There is no
operator action available from `Faulted` that is not available from `Frozen`
(both deny writes; both can only be destroyed). So the exclusion is defensible
on both axes: guest faults *can't* happen to a frozen slot, and host faults
*should* go straight to Destroy.

**Cheapness of a future edge:** adding `Frozen → Faulted` later is nearly free.
The relation is a single `matches!` arm plus one `allowed` tuple in the test;
there are **no stored/persisted transition tables** to migrate (the machine is
pure and computed, `can_transition` is a `const`-ish match). So deferring the
decision costs nothing. Recommend keeping the exclusion as-is but softening the
comment to scope it to guest-contract faults, e.g.: *"a frozen parent executes
nothing, so no determinism-contract fault can originate there; a host-side
integrity failure on a frozen parent is a Destroy (`Frozen → Empty`), not a
Faulted transition."* This pre-empts exactly the devil's-advocate question a
future reader will ask.

## S2. Consider an explicit fault helper rather than ad-hoc `transition(.., Faulted)`

When the producer lands (I1), callers will construct the fault transition with
`slot.transition(SlotState::Faulted)`. Given that two source states
(`Running`, `Paused`) both fault to the same place and a third (`Frozen`)
deliberately must not, a small `fn fault(self) -> Result<SlotState, _>` that
only accepts `Running`/`Paused` would make the legal fault entry points
self-documenting at the call site and prevent a caller from ever passing a
state that the matrix rejects anyway. Pure cosmetics / future-proofing — the
generic `transition` already fails closed, so this is optional.

## S3. `StopReason` "mirrors proto StopReason" comment is now stale

Independent of this diff but adjacent: `runctl.rs:46` says `StopReason`
"mirrors proto StopReason", yet it omits both `NextSdkEvent` and `Faulted`
(and now, with `Faulted` newly emphasized in the SlotState doc, the omission is
more conspicuous). When I1 is addressed, update that comment to state *which*
proto variants are deferred and why, so the mirror claim stays honest.
