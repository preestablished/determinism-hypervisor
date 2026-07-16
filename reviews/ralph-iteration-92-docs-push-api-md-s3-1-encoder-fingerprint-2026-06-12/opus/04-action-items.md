# Action Items

### Critical

_None._ All quoted texts and authority claims verified accurate against the amendment
commits, the `d55ecc3` upstream baseline, and the authoritative source files. The
file is safe to apply upstream as-is.

### Important

_None blocking._

- [ ] [docs/upstream-divergences.md:247-256] For #6, the "Old" quote stops mid-sentence
  while the "Proposed new" replaces a larger span — extend the "Old" anchor to the end
  of defense-item 4 so the upstream patch is mechanically find-and-replaceable like the
  other entries. (Low severity; proposed text is correct, this is about applicability.)

### Suggestions

- [ ] [docs/upstream-divergences.md:247-256] Extend #6 "Old" block to include the
  trailing `(KVM_VCPU_TSC_CTRL offset attribute) over MSR value writes; benchmark both
  in M3 before freezing the mechanism.` clause for a complete replaceable span.
- [ ] [docs/upstream-divergences.md:19-21] Add a one-line quick index mapping
  divergence number → section (amended: #1 #2 #7 #9 #10; upstream-only: #3 #4 #5 #6 #8)
  since entries are not in numeric order within sections.
- [ ] [docs/upstream-divergences.md:149-295] For upstream-only entries (#3, #10
  especially), name the pinning test alongside the authoritative source file so the
  claim is confirmable without reading the whole module.
- [ ] [docs/upstream-divergences.md:86] Add inline byte arithmetic (`56 + 8 + 4 + 4 =
  72`) to the #7 v2 "New" text for parallelism with the #10 NETL row.
