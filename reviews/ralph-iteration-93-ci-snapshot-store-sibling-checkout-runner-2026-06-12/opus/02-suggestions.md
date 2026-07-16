# Suggestions (non-blocking)

### S1 — Note the supply-chain consideration of `@latest` / unpinned installs

- **File / lines:** `docs/ops/github-runner.md:74` (`grpcurl` via
  `go install ...@latest`) and `:75` (`cargo install cargo-fuzz`)
- **What / why:** The `grpcurl` install uses `@latest` and `cargo install
  cargo-fuzz` is unpinned. The table already records the *installed* version
  (v1.9.3, v0.13.2) — which is good — but the *install command* resolves to
  whatever is newest at run time, so a rebuild of this box would not reproduce
  those versions, and an unpinned `@latest` is a supply-chain surface. A short
  caveat would make the table self-documenting about reproducibility.
- **Suggested snippet:** add a fourth bullet under "Notes:":

  ```markdown
  - **Installs are unpinned** (`@latest`, `cargo install` without `--version`).
    The "Status" column records the versions actually on the box; on a rebuild,
    pin to those (`grpcurl/cmd/grpcurl@v1.9.3`, `cargo install cargo-fuzz
    --version 0.13.2`) if you need to reproduce them, or accept the latest.
  ```

### S2 — Anchor the milestone references (M5/M6/M7)

- **File / lines:** `docs/ops/github-runner.md:73–77` ("Needed by" column:
  "M6 smoke tests", "M5 DHILOG fuzz", "M7 soak / chaos load")
- **What / why:** The milestone shorthands are meaningful to current
  contributors but unanchored — a future reader has no link to where M5/M6/M7
  are defined (IMPLEMENTATION-PLAN / testing strategy, which the file's opening
  line already gestures at). One pointer at the section top would let a reader
  resolve them.
- **Suggested snippet:** in the intro paragraph, "Tools the milestone jobs
  (M5–M7, see IMPLEMENTATION-PLAN) need on this box, ...".

### S3 — `stress-ng` candidate version may drift before install

- **File / lines:** `docs/ops/github-runner.md:77` ("candidate
  0.17.06-1build1")
- **What / why:** Recording the apt candidate version is a nice touch, but the
  candidate moves with the archive; by the time an operator runs the
  sudo-gated install it may differ. Minor — consider noting it as "candidate at
  time of writing" so a version mismatch later does not read as an error.
- **Suggested snippet:** `(candidate 0.17.06-1build1 as of 2026-06-12)`.

### S4 — Regex in the grpcurl verify command renders oddly in some viewers

- **File / lines:** `docs/ops/github-runner.md:84`
  (`go version -m ~/go/bin/grpcurl | grep '^\s*mod'`)
- **What / why:** Inside inline code in Markdown the `\s` is fine literally, but
  the `^\s*mod` will also match the `=> mod`/`mod path` lines plus any
  indented `mod` token; in practice `grep '^\s*mod\b'` or `grep -E
  '^\s+mod\s'` is a touch more precise. Purely cosmetic; the command works.
