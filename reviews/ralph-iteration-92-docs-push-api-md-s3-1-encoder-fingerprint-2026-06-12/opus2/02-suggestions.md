# Suggestions (non-blocking)

### S1 — Define "bead" and "iteration" once for the zero-context operator

- **Location:** `docs/upstream-divergences.md:1-18` (intro)
- **Why:** The file is explicitly "for whoever can write to the upstream tree," who "may have
  zero context." Yet entries lean on `bead 4ld`, `bead bcb`, `bead 28i`, `iteration 61`, etc.
  as provenance. An upstream operator outside this repo's ralph/beads workflow cannot resolve
  those, and may not need to — the code/commit citations are the real authority — but a single
  sentence framing them would prevent confusion ("are these things I must look up before I can
  apply this?").
- **Suggested snippet** (add after the first paragraph):
  > "bead" and "iteration N" references below are this repo's internal issue-tracker IDs and
  > development-loop checkpoints; they are provenance only. You do NOT need access to them to
  > apply an entry — the local-amendment commit hash and/or the cited source file are the
  > authority for each change.

### S2 — The applied "new" text for #1/#2/#7/#9/#10 is review-tested; say so explicitly

- **Location:** `docs/upstream-divergences.md:21` (section header for applied amendments)
- **Why:** The prompt context distinguishes the five applied edits (which landed in local docs
  through normal review) from the five proposed wordings (#3/#4/#5/#6/#8) that were never
  review-tested as actual doc text. The file already separates them into two sections, which
  is good — but it does not surface the *confidence asymmetry*. An operator should know the
  first five are byte-for-byte what already passed review locally, while the second five are
  freshly authored here. The section headers ("Divergences with a local amendment" vs
  "Upstream-only wording fixes") imply this but do not state it.
- **Suggested snippet** (append to the first section header line 21):
  > These five "new" texts are verbatim copies of edits that already landed and passed review
  > in this repo's local doc copies (commit cited per entry). The five in the next section are
  > newly-authored proposed wordings — accurate against the cited code, but not yet
  > round-tripped through doc review.

### S3 — Markdown table rows are very long; note the rendering expectation

- **Location:** `docs/upstream-divergences.md:39-40, 86, 142, 163-169` (the `| ... |` rows
  inside fenced code blocks)
- **Why:** The long single-line table rows (e.g. #3's "Proposed new" at line 169 is ~430
  chars, #10's at 142 is ~470 chars) are inside ```` ``` ```` fenced blocks, so they render as
  preformatted text with a horizontal scrollbar rather than wrapping — which is correct,
  because they must be applied verbatim into an upstream Markdown table. That is the right
  choice. A one-line note that these are intentionally single-line (do NOT hard-wrap when
  pasting upstream, or the table cell breaks) would prevent a well-meaning operator from
  reflowing them.
- **Suggested snippet** (near the header): "Table-row replacements are intentionally on one
  physical line; paste them unwrapped — inserting a newline inside a `| ... |` row breaks the
  Markdown cell."

### S4 — #4 proposed-new splits the arrow chain across two inline-code spans; sanity-note it

- **Location:** `docs/upstream-divergences.md:188-189`
- **Why:** The #4 proposed-new turns one backtick-wrapped lifecycle chain into two adjacent
  inline-code spans (`...Running\`, \`Paused → Frozen...`) joined by a comma. This is a
  deliberate way to show "Running" no longer flows directly into "Frozen," and it is
  syntactically valid Markdown. But it is visually subtle, and an operator might "tidy" it
  back into one span, silently reintroducing the bug. A half-line of intent ("the chain is
  split into two code spans on purpose — there is NO `Running → Frozen` edge") would lock it.

### S5 — State the consequence/rollout of applying each entry (ADR "consequences" slot)

- **Location:** throughout (each entry's "Why" bullet)
- **Why:** Best-practice divergence records state decision, context, alternatives, AND
  consequences. The entries capture decision + context + authority well, and several note the
  trigger. What is mostly implicit is the *consequence of applying* (e.g., for #1, once
  upstream is fixed and re-synced, the local amendment `c7e2b1a` becomes a no-op and the bead
  veu can close). A short "After upstream applies + re-sync: local amendment X is subsumed;
  close bead veu" footer per applied entry would make the rollout mechanical and tell the
  operator how to know they're done. (The bead's own notes already say "re-sync .agents/docs
  and close" — mirroring that into the file keeps it self-contained.)
