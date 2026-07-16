# Suggestions (non-blocking)

## S-1. Consider whether the `jq` pipeline is needed at all

**File:** `.github/workflows/ci.yaml:58`

`cargo fmt` **without** `--all` formats the current package and the workspace's
*default members*, not arbitrary path dependencies outside the workspace. The
siblings (`control-plane`, `guest-sdk`) are path *dependencies* but are not
members of this workspace's `[workspace] members` list (root `Cargo.toml`). It is
worth a quick empirical check whether a plain `cargo fmt --check` (run with
`working-directory: repo`) already excludes the siblings — if so, the entire
`cargo metadata | jq | sed | tr` substitution can be replaced by
`cargo fmt --check`, removing the `jq` dependency and the I-1 robustness hazard
outright.

The PR description says `cargo fmt --all` "was observed locally flagging
guest-sdk files." That confirms `--all` reaches them, but does **not** establish
that the no-`--all` form does — those are different code paths in cargo-fmt
(`--all` ≈ "all packages cargo-metadata reports," default ≈ "current/default
package"). A 30-second test (`touch a deliberate misformat in a guest-sdk file,
run plain cargo fmt --check from repo/`) would settle whether the explicit member
list is load-bearing.

## S-2. Pin the fmt scope to `default-members` rather than every metadata package

If the explicit list *is* needed, `cargo metadata --no-deps` lists every package
in the workspace including any future `[[example]]`-only or internal helper
crates that might be added with `publish = false`. Filtering to
`.workspace_default_members[]` (cargo ≥ 1.71 exposes resolved default members in
metadata) or to `.packages[] | select(.source == null)` keeps the intent ("our
crates") explicit and future-proof. Minor; the current `.packages[].name` is fine
for today's 9-member flat workspace.

## S-3. `unwrap_or(i32::MAX)` on parse — document the fail-closed intent inline

**File:** `crates/dh-detclock/src/counter.rs:241`

```rust
let level: i32 = s.trim().parse().unwrap_or(i32::MAX);
```

`i32::MAX` is the right *fail-closed* default: an unparseable sysctl makes
`level <= 1` false, so a non-root user **skips** rather than running a test that
would EACCES. That is correct, but the choice is non-obvious — a reader might
expect `0` (most-permissive) and see a latent "why MAX?" The one-word rationale
("treat an unreadable/garbled level as maximally strict → skip") deserves a
trailing comment so a later refactor doesn't "simplify" it to `unwrap_or(0)` and
silently re-break hosted CI. The surrounding doc comment explains the *policy*
but not this specific sentinel.

## S-4. Counter is a *sampling* event — the `<=1` gate is correct but slightly
conservative is fine

**File:** `crates/dh-detclock/src/counter.rs:245` (informational, no change needed)

The attr is opened as a sampling event (`sample_period = NEVER_FIRES_PERIOD`,
`wakeup_events = 1`). Sampling does not raise the `perf_event_paranoid` gate above
the per-task/`exclude_kernel` rules that already make `<=1` the correct cutoff
(see 03 for the derivation), so no adjustment is needed. Noting it only so a
future reader who adds true overflow sampling (signal delivery via
`route_overflow_to_thread`) knows the paranoid gate was already considered against
the sampling path and is unaffected. Leave as-is.
