# Positive Notes

Patterns in this change worth preserving:

### P1 — Every claim is sourced and verifiable

The "protoc — Not needed" row (`:73`) cites the actual mechanism
(`protoc-bin-vendored`) and decision provenance ("proto-seam decision,
iteration 60"). I verified it directly: `dh-proto/Cargo.toml` and the sibling
crate `../snapshot-store/crates/snapstore-client/Cargo.toml` both depend on
`protoc-bin-vendored`, and `snapstore-client/build.rs` sets `PROTOC` to the
vendored binary path. A reader is told *why* protoc is absent, not just that it
is. This is the gold standard for an ops runbook.

### P2 — The PATH/`.path` mechanic is explained, not just asserted

`:62–69` does not merely say "tools are on PATH"; it explains that jobs inherit
the PATH captured in `.path` at `config.sh` time, names the three directories,
and — crucially — calls out the failure mode and the *correct* remediation
("append the directory to the `.path` file and restart the service" rather than
re-running `config.sh`, which it explicitly flags as "the wrong hammer"). This
prevents a predictable operator mistake.

### P3 — Determinism-class framing is consistent with the rest of the file

The nightly note (`:85–88`) reuses the file's established distinction —
nightly/runner-version are NOT in the determinism class, kernel/microcode are
— and points back to `ci/determinism-class.lock` (verified to exist), mirroring
the identical framing at `:18–20`. The "lane-red, not gate-red" guidance gives
operators a clear triage rule for an inherently unstable toolchain.

### P4 — Honest status tracking with a real open action

The table uses unambiguous status markers (✅ installed with version, ❌
pending) and the `stress-ng` note (`:89–91`) is refreshingly honest about the
one thing automation *cannot* do (apt needs sudo, which the box's automation
lacks) plus the exact post-install verification. This turns the doc into a live
checklist rather than aspirational prose.

### P5 — The grpcurl version-stamping gotcha is documented

`:81–84` pre-empts a confusing observation — `grpcurl --version` printing
`dev build <no version set>` for `go install` binaries — and supplies the real
verification command. This is exactly the kind of hard-won, non-obvious detail
a runbook exists to capture.
