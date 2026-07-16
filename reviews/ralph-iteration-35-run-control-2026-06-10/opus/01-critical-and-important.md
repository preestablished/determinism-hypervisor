# Critical & Important findings

## CRITICAL

### C1 — Multi-vector injection at one boundary silently drops all but the last vector

**File:** `crates/dh-vmm/src/runctl.rs` lines 191-212; root cause in
`crates/dh-vmm/src/inject.rs` (`inject_at_boundary` injectable-true path, lines 137-146)
and the `queue_interrupt` kernel contract (lines 96-99).

**What the code does.** A single `StopPoint` can carry more than one injection index
(`agenda.rs` `StopPoint::injections: Vec<usize>`; the `prop_*` test generates up to 20
injections and the merge explicitly stacks coincident ones). run_segment walks them in a
tight inner loop:

```rust
let mut at = boundary;
for idx in &point.injections {
    let inj = inject_at_boundary(&mut seg.slot.vcpu, seg.counter,
                                 seg.injections[*idx].vector, &at, &margins,
                                 seg.config.epoch_len, on_exit)?;
    delivered += 1;
    at = Boundary { icount: inj.delivered_icount, rip: inj.delivered_rip, rcx: at.rcx };
}
```

**Why it is broken.** When the vCPU is injectable at the boundary (the common case at a
fresh landing), `inject_at_boundary` takes the injectable-true branch:

```rust
if injectable(vcpu)? { queue_interrupt(vcpu, vector)?; break Ok(Injection{..}); }
```

It **queues and returns without ever entering the guest** (`land_at` is only called on
the *deferral* path, when the window is closed). So between the first and second
iteration of run_segment's loop:

1. iter 1: `injectable()` reads `kvm_run` (true) → `KVM_INTERRUPT(v0)` queued → returns.
   `at` is updated, **but no `KVM_RUN` has executed**.
2. iter 2: `inject_at_boundary` re-enters with `current = at` (same icount). `injectable()`
   reads the **same stale `kvm_run`** — `ready_for_interrupt_injection` is still 1
   because no entry consumed the queued vector — returns true → `KVM_INTERRUPT(v1)`.

`inject.rs` documents the kernel behavior precisely (lines 96-99): *"a second
KVM_INTERRUPT before the next KVM_RUN silently OVERWRITES the first on this kernel (no
EEXIST; verified live in review). Run control must enter the guest between injections."*

**run control does NOT enter the guest between injections.** `v1` overwrites `v0`; `v0`
is lost. `delivered` is nevertheless incremented to 2, so the AUX/outcome count lies.

**The comment in runctl.rs is false.** Lines 191-193 claim: *"one vector per entry —
`inject_at_boundary` steps between queued vectors when several share a boundary, so each
gets its own VM entry."* `inject_at_boundary` only steps when **deferring** a closed
window; on the injectable-true path it never steps. Two injectable vectors at one
boundary → one VM entry → one surviving vector.

**Severity rationale — why Critical, not just latent.** This is a determinism platform.
A dropped timer/IPI vector is not a crash; it is a **silent wrong-result** that the
state-hash chain will faithfully reproduce (both replays drop the same vector), so
verification mode will *not* flag it — the bug masquerades as deterministic correctness.
The agenda layer is explicitly designed to deliver this case (the `StopPoint::injections`
doc, §3.4 "run control must chain them across consecutive entries"), so the gap is
structural, not theoretical. Phase-1's only live caller passes `injections: &[]`, which
is exactly why the green test suite says nothing about it.

**Fix options (pick one):**

1. **Enter the guest one retirement between queued vectors.** After each `queue_interrupt`,
   `land_at(at.icount + 1)` to consume the queued vector before the next `injectable()`
   check. This matches §3.4's "delivered on the next KVM_RUN entry, before any guest
   instruction retires" — the second vector's true delivery boundary is then `at+1`, not
   `at`, which is the correct deterministic answer. Update `at` from the post-entry boundary.
2. **Have `inject_at_boundary` itself enter the guest after queueing** (push the "one
   vector per entry" contract down into the function that owns it) and return the
   post-delivery boundary. Cleaner ownership; touches the inject API.
3. **Minimum viable for Phase-1:** if you are not ready to fix delivery semantics, reject
   `point.injections.len() > 1` with a loud `RunError` so the lost-vector case can never
   ship silently. Then fix properly when the device loop lands.

**Required test (add now):** a live test that schedules two vectors at one boundary on a
guest with IF=1 and asserts BOTH deliver (e.g. through an IDT that records each vector,
or two distinct observable effects) and that the second's `delivered_icount` is the first's
`+1`. Run it twice for replay identity. The existing `inject.rs` single-vector live tests
prove the primitive; this proves the chaining run control claims to do.

---

## IMPORTANT

### I1 — `hash_epochs = FinalOnly` is silently ignored; epochs always hashed

**File:** `crates/dh-vmm/src/runctl.rs` lines 170-176.

run_segment unconditionally passes the epoch grid to the agenda:

```rust
epoch_len: std::num::NonZeroU64::new(seg.config.epoch_len),
```

It never consults `seg.config.hash_epochs`. `config.rs` defines `HashEpochs::{EpochsOn,
FinalOnly}`, ARCH §10 documents `hash_epochs=final_only` as a real exploration-mode knob
("exploration jobs only need the final state hash"), and — critically — **`hash_epochs`
is part of the `machine_config_hash` preimage** (config.rs lines 187-190, and the
`hash_stable_and_field_sensitive` test asserts flipping it changes the config hash).

Consequence: a config declaring `FinalOnly` advertises (via H_0, which folds
`machine_config_hash`) that its chain has no epoch links, but the engine still inserts
them. The chain a `FinalOnly` config actually produces does not match what a conforming
implementation — or this same code once it honors the flag — would produce. It is
internally deterministic, but it is **wrong against the config's own declared contract**
and an interop divergence waiting to happen.

For Phase-1 the default is `EpochsOn`, so the live path is correct today; this is why no
test catches it. Fix is one line plus a test:

```rust
epoch_len: match seg.config.hash_epochs {
    HashEpochs::EpochsOn => NonZeroU64::new(seg.config.epoch_len),
    HashEpochs::FinalOnly => None,
},
```

Note: with `FinalOnly`, the pause roll-forward (lines 240-241) loses its epoch grid — see
S1; the two must be designed together. (The agenda already tolerates `epoch_len: None`.)

### I2 — Coincident epoch + final-stop boundary produces TWO chain links, not one

**File:** `crates/dh-vmm/src/runctl.rs` lines 218-223 (epoch arm) + `finish()` lines
277-279.

When an agenda point is **both** `epoch_hash == true` **and** carries `final_stop`
(entirely possible: e.g. `IcountBudget` whose final icount is a multiple of `epoch_len`;
the agenda's `merges_all_sources_sorted` test shows exactly such a coincident point at
3000), run_segment:

1. takes the `if point.epoch_hash` arm → `push_final_link(...)` (link A at this boundary),
   then
2. falls through to the `final_stop` arm → `finish()` → `push_final_link(...)` **again**
   (link B at the same boundary, identical preimage except it folds A's value).

§8.5 defines the chain as hashed *"at every epoch boundary and at every final pause"* —
a boundary that is both is **one** hash point, not two. A conforming peer (or this code
after a fix) hashing once at that coincident boundary computes a **different chain value**
from the first divergent epoch onward. Since "the chain value is the state hash exchanged
with other services" (§8.5), two implementations that disagree on whether a coincident
boundary is one link or two will never match — this is a cross-implementation interop bug,
not a cosmetic one, even though each side is internally deterministic.

The pause branch already pushes its own link and returns directly (does not call
`finish()`), so it is single-linked and fine. The defect is specifically the
epoch-arm-then-finish double push.

**Fix:** make `finish()` skip its link when the stop boundary was already hashed this
iteration, or restructure so the final point hashes exactly once. Concretely: track
whether the current point already hashed and pass that into `finish()`, or move the
final hash so it is emitted once per stop point regardless of how many actions coincide.
Add a test asserting a budget landing exactly on an epoch multiple yields the same chain
value as the same run with the budget one instruction short of the multiple plus a manual
final hash (i.e. link count is what §8.5 prescribes).
