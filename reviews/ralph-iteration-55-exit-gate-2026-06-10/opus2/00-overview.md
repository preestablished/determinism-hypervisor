# Phase 1 Exit Gate Sign-off — Second Independent Review

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-55-exit-gate`
- **Diff:** `git diff main...HEAD` — `docs/phase-1-exit-gate.md` (new) + `docs/ops/skid-histogram-2026-06-10.txt` (new), 60 insertions, 0 deletions.
- **Verdict:** **APPROVE** (with one Important documentation gap to fix in-doc; not a blocker for the gate's correctness)

## Scope of this review

I reviewed the sign-off as (1) a *completeness claim* against the Phase 1 exit
gate and the M0–M3 acceptance criteria, (2) a *handoff document* for the M4
implementer, (3) the archived skid histogram artifact, (4) the bd graph state,
and (5) the operational sequencing process. I did not re-audit every evidence
row by re-execution — that is reviewer 1's angle. I ran a verification subset of
my own choosing (below) to confirm the box and the cited tests are real.

## Headline finding

The sign-off is **substantively correct and well-evidenced for the hypervisor
critical path** (gate criteria 1 and 2, plus M0–M3 acceptance). Every "staged
machinery" claim in the "What this unblocks" section grep-verifies against real,
documented code — this is an unusually honest and useful handoff.

**The one real gap is documentary, not technical:** the Phase 1 exit gate as
written in `.agents/docs/phases/phase-1-deterministic-execution.md` has **four**
criteria. This sign-off covers gate criterion 1 (determinism) and 2 (landing) in
full, but is **silent** on gate criterion 3 (snapshot-store M1/M2 benchmark
gates on synthetic data) and gate criterion 4 (guest-sdk agent boots in-guest
and streams log events). Those two tracks live in **sibling repos**
(`../snapshot-store` does not exist as a path dep; `../guest-sdk` is a path dep
for `detguest-host`/`detguest-wire` only), so it is entirely defensible that
this hypervisor-repo sign-off does not test them — but the document never *says*
that. A reader who only has the phase doc and this sign-off cannot tell whether
criteria 3 and 4 are (a) out of scope for this repo's gate, (b) deferred, or (c)
forgotten. That ambiguity is the finding. See `01-critical-and-important.md`.

This does **not** block the *narrow* sequencing guard the doc actually invokes —
"hypervisor M4 (snapshots) MUST NOT start until [the *determinism* gate] is
green." M4 is gated on determinism + landing being proven, and those *are*
proven here. So APPROVE stands; the fix is one or two sentences of scope text.

## Verification subset I ran (lab box `infra-control`)

- `/dev/kvm` present (`crw-rw---- root kvm`); host is `infra-control` as the doc states.
- `cargo test -p determinism-tests --test counting_semantics` → **2/2 ok** (matches table row 4).
- `cargo test -p determinism-tests --test m1_acceptance` → **1/1 ok** (matches table row 5's `m1_acceptance`).
- `cargo run -p dh-cli -- skid --samples 2000` → reproduces the histogram shape
  (mode at buckets 27/30/31, max 54 on this short run, `GATE OK ... < 4096`) —
  consistent with the archived 50k-sample run and the README's stochastic-tail framing.
- Tree clean after all runs (only `target/` rebuild artifacts; no tracked changes).

## File map for this review

- `01-critical-and-important.md` — the negative-space gap + bd/process observations.
- `02-suggestions.md` — handoff polish, operational gating suggestion.
- `03-positive-notes.md` — what this sign-off does well (verified).
- `04-action-items.md` — Critical / Important / Suggestions, self-contained.
