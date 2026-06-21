# Resolve 3l2 VerifyReplay Bisection Diagnostics Closeout

Plan name: `resolve-3l2-bisection-diagnostics-closeout`

Selected bead: `determinism-hypervisor-3l2` - M8 VerifyReplay divergence
bisection diagnostics.

## Why This Bead

`determinism-hypervisor-3l2` is still marked `BLOCKED`, but its recorded local
blocker appears to have been removed. The seven unblock beads listed under
`DEPENDS ON` are all closed:

- `3l2.1`: DHILOG `BISECTION_CHECKPOINT` AUX codec.
- `3l2.2`: non-mutating bisection checkpoint snapshot capture.
- `3l2.3`: recorder checkpoint scheduling and AUX emission.
- `3l2.4`: VerifyReplay checkpoint indexing and selection.
- `3l2.5`: snapshot comparison utilities.
- `3l2.6`: replay probe divergence construction.
- `3l2.7`: end-to-end VerifyReplay bisection evidence tests.

Unlike `determinism-hypervisor-veu`, this bead is local to this repository and
does not depend on an unreachable upstream planning tree. The current Linux/KVM
reference host can run the KVM-backed validation needed for a defensible
closeout.

## Intended Outcome

The coding agent should treat completion as unproven until it performs the
audit in this plan. If the audit confirms current code satisfies every parent
acceptance requirement, the agent should unblock and close `3l2` with evidence.
If the audit finds any gap, the agent should implement the narrow missing fix,
rerun the relevant gates, then close.

This is not a plan to reimplement all seven child beads. The likely fix is a
completion/status repair plus verification. The plan still includes concrete
patch paths because the audit may reveal stale, partial, or regressed behavior.

## Parent Acceptance Requirements

From `bd show determinism-hypervisor-3l2`, the parent requires:

- `VerifyReplay` with `bisect_on_divergence=true` performs true divergence
  bisection instead of returning the phase-1 coarse fallback.
- The public `Divergence` proto fields are populated from evidence:
  `icount_lo`, `icount_hi`, `rip_expected`, `rip_actual`, postcard-encoded
  `reg_diff`, and `diff_page_idx`.
- Coarse/fabricated evidence is not emitted as refined bisection diagnostics.
- Service-level divergent replay tests cover both `bisect=true` and
  `bisect=false`.

## Desired End State

- `determinism-hypervisor-3l2` is unblocked, claimed if needed, audited,
  fixed if needed, commented with evidence, and closed.
- Code-level audit confirms the replay path uses checkpoint evidence for
  bisection diagnostics and fails closed when evidence is absent or invalid.
- Tests prove checkpointed divergence, checkpoint-less failure, coarse
  `bisect=false`, invalid checkpoint metadata, and diagnostic field population.
- Reference-host KVM validation runs on this Linux/KVM machine.
- Beads and Git are pushed before handoff.

## File Map

- `01-current-state.md` records current Beads and code evidence.
- `02-requirement-audit.md` defines the completion audit.
- `03-gap-fix-playbook.md` lists concrete implementation patches if the audit
  finds a missing requirement.
- `04-validation-reference-host.md` gives the focused and reference-host gates.
- `05-beads-and-closeout.md` gives the Beads/Git closeout sequence.
- `06-review-resolution.md` summarizes accepted subagent review feedback.
- `07-review-correctness.md` and `08-review-reference-host.md` contain the two
  independent plan reviews.
