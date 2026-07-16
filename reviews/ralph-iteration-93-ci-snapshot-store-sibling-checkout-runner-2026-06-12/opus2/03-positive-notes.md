# Positive Notes (patterns worth preserving)

### P-1 — Every factual claim survives independent verification
I checked the load-bearing claims against the live box and the repo, and they all hold:
- `protoc` "Not needed": both `crates/dh-proto/build.rs:6` **and** the sibling
  `../snapshot-store/crates/snapstore-client/build.rs:3` set `PROTOC` via
  `protoc_bin_vendored::protoc_bin_path()` — the doc's claim that *both* crates vendor it is
  literally true, not a hand-wave. It also matches `docs/decisions/proto-seam.md:24-25,35-38,50`
  ("runner provisioning is a no-op").
- `grpcurl` really prints `dev build <no version set>` on this box, and the documented
  recovery `go version -m ~/go/bin/grpcurl | grep '^\s*mod'` really returns `v1.9.3` — this is
  the kind of "non-obvious gotcha + the exact command that defeats it" that saves an operator
  twenty minutes.
- `cargo-fuzz 0.13.2`, nightly installed, `stress-ng` genuinely absent — table matches reality.
- `ci/determinism-class.lock` exists and confirms kernel/microcode as the determinism class,
  exactly as the nightly caveat and the I-2 cross-reference assert.

This is a higher bar of accuracy than most runbook prose clears. Preserve the habit of
recording the *exact observed version* in the Status column.

### P-2 — The `.path` vs `config.sh` distinction is the genuinely valuable insight
Lines 66-69 — "if a tool is installed to a directory NOT on that captured PATH, re-running
`config.sh` is the wrong hammer — append the directory to the `.path` file and restart the
service" — is exactly the trap an operator falls into (reaching for the big reconfigure when
the fix is a one-line file append). Calling out the wrong move *and* the right one is good
runbook craft.

### P-3 — Correct, load-bearing scoping of nightly out of the determinism class
Lines 85-88 get a subtle thing right: nightly Rust auto-updates and therefore must be
"lane-red, not gate-red," and the doc correctly anchors this to `ci/determinism-class.lock`
(kernel/microcode only). Conflating toolchain drift with determinism-class drift would be a
real correctness bug in how CI failures are triaged; the doc pre-empts it. This is the
edge-case reasoning most reviewers would skim past.

### P-4 — Each tool row carries its own "why" and provenance
"Needed by M5/M6/M7" + the proto-seam/iteration-60 attribution on the protoc row means a
future reader knows not just *what* to install but *which milestone breaks without it* and
*which decision made protoc unnecessary*. The M5/M6/M7 references check out against
`docs/prompts/phase-2-of-determinism-hypervisor-fork-and-replay.md` (M5 = DHILOG codec +
`cargo fuzz` target; M6 = worker daemon gRPC :7400; M7 = fork-1000× soak). Provenance-per-row
is worth keeping as the table grows.
