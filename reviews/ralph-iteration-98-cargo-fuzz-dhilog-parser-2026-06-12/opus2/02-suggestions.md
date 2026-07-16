# Suggestions (non-blocking)

## S1 — Corpus does not persist across nightlies; every run restarts from 2 seeds
`fuzz/corpus/` is `.gitignore`d (`fuzz/.gitignore:2`) and the job does not cache it, so each scheduled run seeds *only* from `tests/fixtures` (2 golden `.dhilog` files) plus whatever libFuzzer generates that hour — then discards it. Coverage resets nightly; there is no corpus growth across runs, so the 1h nightly never compounds.

I verified the reset is real: in a clean checkout `fuzz/corpus/dhilog_parse` does not exist (cargo-fuzz creates it empty), and INITED coverage is whatever the 2 fixtures reach. A persisted corpus would let each night start from the union of all prior discoveries.

**Suggested fix:** cache the corpus dir keyed on the target name, e.g.

```yaml
- uses: actions/cache@v4
  with:
    path: repo/crates/dh-inputlog/fuzz/corpus/dhilog_parse
    key: dhilog-fuzz-corpus-${{ github.run_id }}
    restore-keys: dhilog-fuzz-corpus-
```

(`run_id` in the key + `restore-keys` prefix gives append-on-each-run accumulation.)

**Security weighing (this repo's posture):** public repo, but Actions cache is *branch-scoped* and scheduled runs execute on the default branch only — a cache written by `nightly-drift` on `main` is only restorable by workflows running on `main`. A PR fork cannot write into the default-branch cache scope. The corpus is opaque fuzz inputs fed only to a sandboxed libFuzzer binary, not executed as code, so cache-poisoning blast radius is "fuzzer wastes time on junk inputs," not RCE. The risk is acceptable; the coverage win is real. **Recommend doing it**, but it is correctly out of scope for this bring-up commit — file as a follow-up bead.

## S2 — `cargo install cargo-fuzz --locked` adds ~2–4 min/run
Cheap relative to a 1h (or 24h) fuzz budget, so not worth blocking on. If trimmed at all, prefer `taiki-e/install-action@cargo-fuzz` (downloads a prebuilt binary, ~seconds) over caching the compiled artifact:

```yaml
- uses: taiki-e/install-action@v2
  with: { tool: cargo-fuzz }
```

Only adopt if you're already standardizing on `taiki-e/install-action` elsewhere; otherwise the explicit `cargo install` is the most transparent option and fine as-is.

## S3 — Document the schedule-vs-dispatch interference more precisely
The 24h operator dispatch (`fuzz_runner=kvm-intel`) holds concurrency group `kvm-intel-nightly-drift` (`nightly-drift.yaml:29–31`, `cancel-in-progress: false`) for the whole run. GitHub's real behavior during that window:
- The 03:17 **scheduled** run does not cancel the dispatch (good — `cancel-in-progress: false`), and is instead held **pending** for up to ~24h, then runs late.
- GitHub keeps **at most one pending run per group**; a newer pending replaces the older. With a daily cron and a <25h window only one nightly ever queues, so nothing is silently dropped *in this configuration* — but the drift/canary measurement is **delayed by up to a full day** behind the dispatch.

The current comment ("schedule it deliberately") implies operator caution but doesn't state the consequence: **running the 24h accept dispatch blanks out timely drift/canary detection for ~24h.** Suggest tightening the comment near `nightly-drift.yaml:14–17` to say so explicitly, e.g. "a 24h kvm-intel dispatch holds the concurrency group; the scheduled 03:17 nightly will queue behind it and run late — do not start a 24h dispatch if a fresh drift reading is due."

## S4 — `splice.rs` read path is untargeted
The fuzz target covers `LogReader::parse` + accessors but not `Lineage::new` / `extend` / `edges` (`splice.rs`), which re-parse and cross-check multiple segments. The interesting splice-specific logic (stitch comparison, `parsed.len() - 1` loop bound at `splice.rs:85`, the `index - 1` arithmetic in `extend` at `splice.rs:113–118`) is not hostile-input-driven here. A second `lineage_splice` fuzz target (arbitrary segment vector via `arbitrary`/length-prefixed framing) is a reasonable **follow-up bead**, not this commit's scope. Note `splice.rs:113` `self.segments[index - 1]` is safe only because `Lineage::new` rejects empty and `extend` always has ≥1 segment — worth a fuzz target to keep it honest as the type evolves.

## S5 — Typo'd `fuzz_runner` label queues forever
If an operator dispatches with a `fuzz_runner` value that matches no runner label (e.g. `kvm-intel`), the job queues until `timeout-minutes` (1500 → 25h) with no runner ever picking it up, and the run shows as "in progress" the whole time — also holding the concurrency group and blocking the nightly (see S3). Worth a one-line note in `docs/ops/github-runner.md` or the input `description` that the only valid `fuzz_runner` values are `ubuntu-latest` and `kvm-intel`, and a typo strands the run. Minor; documentation-only.
