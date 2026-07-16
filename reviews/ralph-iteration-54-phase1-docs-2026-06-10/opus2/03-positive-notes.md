# Positive notes

Verified independently on the lab box; these all hold up:

- **Every measured number traces to source and matches.**
  - `skid_margin = 8192` → `crates/dh-vmm/src/config.rs:85` (`DEFAULT_SKID_MARGIN`). ✓
  - "gate alerts at margin/2" → `tools/dh-cli/src/cli.rs:70-72` ("max skid < skid_margin/2")
    and `tools/dh-cli/src/skid.rs:2` ("gate it against skid_margin / 2 (risk R1 alert threshold)"). ✓
  - ">200× headroom": 8192/39 = 210. Arithmetic correct. ✓
  - TSC 932 / 1107 → `docs/decisions/tsc-alignment.md:24-25`, byte-for-byte. ✓
    (one labeling nit captured in 01).

- **dh-cli subcommand list is exact.** All six (caps, cpuid-diff, boot, run,
  skid, gate) and their flag syntax match `tools/dh-cli/src/cli.rs` dispatch
  (lines 32-86) and the in-binary `usage()` string. Nice that the doc mirrors
  the real usage text rather than paraphrasing.

- **Test-partitioning table file names are all real and correctly placed.**
  Cross-checked every command: `dh-vmm --test blk_fixture`, `nanokernel --test
  channel_interop`, `dh-cli --test boot_hello`/`skid_gate`, and the
  `determinism-tests` suite (regression, timer_determinism, if0_deferral,
  counting_semantics, counting_smoke, landing_precision, m1_acceptance) — all
  exist as named. The package name `determinism-tests` matches
  `tests/determinism/Cargo.toml`. No phantom commands.

- **Ran a verification subset clean:**
  - `dh-worker --lib preflight` (6 tests) ✓, `dh-vmm --test blk_fixture` (3) ✓
    — the two host-runnable rows.
  - `dh-cli --test boot_hello` (6) ✓, `--test skid_gate` (2) ✓,
    `determinism-tests --test counting_semantics --test counting_smoke` (3) ✓
    — gated rows, run live on the box.
  - `cargo run -p dh-worker --bin dh-workerd -- --preflight` → 17/17 ok,
    exit 0 — runbook step 2 works verbatim.
  - `bash ci/check-determinism-class.sh` → "matches the lock (7 keys)", exit 0
    — runbook step 3's "7/7 keys ok" is accurate (the lock has exactly 7 keys).
  - Tree clean after (`git status --short` empty).

- **R2 section is precise and honest.** The counting-semantics rule (plain +1;
  REP MOVSB once; VM-exiting instructions retire zero under `exclude_host=1`)
  matches what `counting_semantics.rs` asserts, and the README candidly records
  that the original spec's "exactly once" was *refuted* by the empirics and the
  vendored ARCH §3.1 was corrected. That's the right way to document a
  measured surprise. The BR_INST_RETIRED fallback being framed as "not needed;
  trigger is a future bump failure" is consistent with CONTRIBUTING.md:18-22.

- **Crate dispositions (hny) are complete and consistent.** README §"Workspace
  layout" + "Disposition of the initial scaffold-only crates" cover dh-types
  (folded into dh-vmm), dh-kvm (folded into dh-vmm), dh-smoke (retired into
  dh-worker tests). All three accounted for, matching the bead ask.

- **Cross-doc policy consistency holds where it counts.** skid_margin,
  margin/2 alert, the 7-key lock, the required-check posture, and the
  re-baseline-is-a-procedure framing are stated identically (or compatibly)
  across README / CONTRIBUTING / test-partitioning / host-config-intel-box /
  nightly-drift. No contradictory numbers found (the only labeling slip is the
  TSC "alignment error" wording in 01).

- **The doc honestly markets the self-skip design** ("same command is correct
  everywhere") — and it's true: I confirmed the gated legs guard on an rw
  `/dev/kvm` probe (preflight.rs:179-193, and the same skip-vs-fail split in
  the full_preflight test), so `cargo test --workspace` really is the one
  correct invocation on every host class.
