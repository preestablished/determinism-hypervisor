# Suggestions

### S-1. `Frozen→Faulted` omission: the doc rationale only covers *guest-side* faults — there is a plausible *host-side* fault class on a frozen parent

**Where:** `lib.rs:100-103` doc, and `faulted_is_terminal_short_of_destroy`
asserts `!Frozen.can_transition(Faulted)` (line 308).

The documented justification is: *"a frozen parent executes nothing and accepts
no writes, so nothing can fault it."* That reasoning is airtight for the
**determinism-contract** fault class (guest-contract violation, log DATA_LOSS,
divergence) — those all require the guest to *run*, and a frozen parent does
not. The `ensure_write_path(Frozen)` denial (line 144) backs this up: no write
path can mutate a frozen slot into an inconsistent state.

But "frozen" is not the same as "inert at the host level." A frozen parent
still owns live host resources: a KVM vm/vcpu fd, the sealed memfd backing its
shared CoW baseline, and that baseline is *read* by every child. A host-side
failure on the frozen parent — e.g. the seal read-back / integrity check
failing, the memfd becoming unreadable, or a KVM fd going bad — is a real fault
class that the doc's "executes nothing" framing does not address. Today such a
failure has nowhere to go but `Frozen→Empty` (Destroy), which is arguably
*wrong*: destroying a frozen parent while it still has CoW children would pull
the shared baseline out from under them. A `Frozen→Faulted` edge that *parks*
the parent (children quiesced, no resurrection) may be the more correct sink.

**This is genuinely revisitable cheaply** — and the diff should say so. The
transition relation is a pure in-memory `matches!` expression over
`(SlotState, SlotState)` with zero persisted state, zero wire contract, and
zero callers wired in yet (`ensure_write_path` doc, line 137, notes no engine
calls it). Adding `(Frozen, Faulted)` later is a one-line `matches!` arm plus
one test row — no migration, no compatibility concern. I'm **not** recommending
adding the edge now (no caller needs it, and YAGNI applies to a state machine
with no live consumers). I *am* recommending the doc comment be honest about
*scope*: change "nothing can fault it" to "no **guest-contract** fault can
reach it; a host-side fault on a frozen parent is out of scope for this
iteration and, if needed, is a one-line addition," and capture the host-fault
question in a follow-up bead. As written, the doc reads as a closed proof when
it is actually a scoping decision.

### S-2. The 5×5 matrix test is a tautology of the relation; the terminality test is *mostly* a restatement too — add one genuinely independent property

**Where:** `transition_matrix_is_exactly_the_spec_relation` (lines 230-265) and
`faulted_is_terminal_short_of_destroy` (lines 300-310).

The matrix test derives its oracle (`allowed`) from a literal list that is a
1:1 transcription of the `matches!` arms in `can_transition`. It cannot catch a
logic error in the *relation* — only a divergence between the list and the
`matches!` body (i.e. an edit that touches one but not the other). That drift-
detection value is real and worth keeping, but it is the *only* value; the test
proves nothing about whether the relation is *correct*, only that two copies of
it agree.

The new `faulted_is_terminal_short_of_destroy` test is honest about being a
terminality check, but its first loop (lines 304-306) asserts the exact same
four non-edges the matrix already covers — a restatement. Its real signal is in
the two trailing lines (`!Frozen.can_transition(Faulted)`,
`!Empty.can_transition(Faulted)`), which name *specific* design decisions.

**Suggestion:** add one or two *structural* properties that are derived from the
state machine's *meaning*, not from the edge list, so they would catch a wrong
edge even if the `allowed` list were edited to match it:

- **Every non-Empty state can reach `Empty`** (the machine has no dead-end other
  than via Empty): `for s in ALL where s != Empty { assert!(exists path to Empty) }`
  — or at minimum the one-hop version for the terminal/near-terminal states.
- **`Faulted` is reachable only from `Running` and `Paused`** stated as a
  *predecessor* property: `for s in ALL { assert_eq!(s.can_transition(Faulted),
  matches!(s, Running | Paused)) }`. This expresses "what can fault" as a
  closed-form predicate independent of the `allowed` tuple list, so an erroneous
  extra fault edge (e.g. someone adding `Empty→Faulted`) fails here even if they
  also added it to `allowed`.

These cost a few lines and convert "two transcriptions agree" into "the relation
has the shape the design claims."

### S-3. `Faulted` placed last in the enum — confirm no code relies on `SlotState` discriminant values

Adding `Faulted` after `Frozen` keeps existing discriminants (`Empty=0` …
`Frozen=3`) stable, which is the safe choice and clearly intentional. I
confirmed no `as i32`/`as u8` cast against the domain enum exists in `crates/`,
so nothing breaks. Worth a one-line note in the bead that the variant was
appended *specifically* to preserve discriminants, so a future contributor does
not "tidy" the enum into alphabetical/lifecycle order and silently renumber it.
(Ties into I-1: the moment a proto mapping is written by-match, discriminant
order stops mattering, but until then, append-only is the rule.)

### S-4. `FaultedWriteDenied` vs `EmptyWriteDenied`/`FrozenWriteDenied` — consider whether a single `WriteDenied { state, api }` would scale better

Minor / stylistic. There are now three near-identical write-denial variants
distinguished only by the originating state. As more states or denial reasons
accrue, a single `WriteDenied { state: SlotState, api: &'static str }` carrier
would collapse the `ensure_write_path` match into `Err(WriteDenied { state:
self, api })` for the deny arms and remove the per-state variant proliferation.
The current explicit-per-state form has the virtue that the *type* names the
violation (a `FrozenWriteDenied` is grep-able and self-documenting at the R9
guard), so this is a genuine trade-off, not a clear win — flagging it only so
the choice is deliberate. No change required.
