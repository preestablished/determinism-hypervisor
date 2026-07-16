# Suggestions (non-blocking)

### 1. Make the #6 "Old" anchor quote-complete

`docs/upstream-divergences.md:247-256`

The "Old" block for #6 truncates the upstream caveat sentence at `prefer adjusting
the **TSC offset**`, while the "Proposed new" rewrites the whole of defense-item 4
including the trailing `(KVM_VCPU_TSC_CTRL offset attribute) over MSR value writes;
benchmark both in M3 before freezing the mechanism.` clause that the quote never
shows. Extend the "Old" quote to the end of item 4 so the applier can do an exact
find-and-replace of the full item. Suggested addition to the "Old" block:

```
     (`KVM_VCPU_TSC_CTRL` offset attribute) over MSR value writes; benchmark both in
     M3 before freezing the mechanism.
```

(Every other entry already quotes a complete, replaceable span — this brings #6 in
line.)

### 2. Order entries by divergence number within each section

`docs/upstream-divergences.md:21-145` and `147-295`

The two sections are correctly grouped by "has local amendment" vs "upstream-only",
but within each group the numbers run non-monotonically (#1, #2, #7, #9, #10 / then
#3, #4, #5, #6, #8). A reader cross-referencing a number from the bead has to scan
both sections. Consider adding a one-line index at the top mapping number → section,
e.g.:

```
Quick index: amended locally — #1 #2 #7 #9 #10; upstream-only — #3 #4 #5 #6 #8.
```

This costs one line and removes the hunt. (Reordering the entries themselves is not
worth the churn given the numbers encode discovery order.)

### 3. State the authority commit/test for each upstream-only entry's *evidence*, not just the source file

`docs/upstream-divergences.md:149-295`

The upstream-only entries (#3–#6, #8) name the authoritative *file* (e.g.
`detchannel.rs`, `dirty.rs`) but, unlike the amended entries, carry no commit hash or
test name as the load-bearing evidence anchor. For #3 and #10 the constants
(`EVTC_LEN`, the 36-byte NETL) are pinned by tests; naming the pinning test (e.g. the
golden/round-trip test file) would let an upstream maintainer confirm the claim
without reading the whole module. Minor — the file references already make the claim
checkable; this just shortens the check.

### 4. Spell out the v2 byte arithmetic inline for #7

`docs/upstream-divergences.md:86`

The #7 "New" text says "72 bytes total" for v2. Adding the arithmetic
(`56 + 8 + 4 + 4 = 72`) inline, as #10 does for NETL (`36 bytes: ...`), would let a
reader sanity-check the total without summing field widths themselves. Purely a
parallelism/clarity nit.
