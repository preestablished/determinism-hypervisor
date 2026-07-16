# Critical & Important Issues

**None.**

I checked every item flagged for scrutiny and each one holds up:

### Self-hosted fork-PR guard — correct
`ci.yaml:49`
```yaml
if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository
```
This is exactly the pattern recommended in the security research
(`github-actions-selfhosted-security.md`). Case analysis:
- **Same-repo push** → `github.event_name == 'push'` is true → runs. Correct.
- **Same-repo PR** → not a push, but `pull_request.head.repo.full_name`
  (`preestablished/determinism-hypervisor`) `== github.repository` → runs. Correct.
- **Fork PR** → not a push, and `head.repo.full_name` is the fork's slug
  (`someuser/determinism-hypervisor`) `!= github.repository` → skipped.
  The self-hosted box is never reached by untrusted forks. Correct.

No premise breaks the gate: on a `push` event `github.event.pull_request` is
null, but short-circuit `||` means the second operand is never evaluated, so
there is no null-deref/false-string comparison footgun.

### guest-sdk checkout — correct and is the actual unblock
`ci.yaml:27-30, 59-62`. The workspace root `Cargo.toml` declares
`detguest-host = { path = "../guest-sdk/crates/detguest-host" }` and
`detguest-wire = { path = "../guest-sdk/crates/detguest-wire" }`. CI checks the
three repos out as siblings (`repo/`, `control-plane/`, `guest-sdk/`), so from
`repo/Cargo.toml` the path `../guest-sdk/crates/detguest-host` resolves to
`<workspace>/guest-sdk/crates/detguest-host`. Depth is correct. I confirmed
`preestablished/guest-sdk` and `preestablished/control-plane` are both PUBLIC,
so the default `GITHUB_TOKEN` (or anonymous fetch) can clone them with no PAT —
identical to how `control-plane` was already checked out on `main`.

### `% n != 0` → `!is_multiple_of(n)` — semantics preserved
`agenda.rs:408-410`. `XorShift::next()` returns `u64` (`agenda.rs:195`). For an
unsigned integer, `x % 4 != 0` is exactly `!x.is_multiple_of(4)` and
`x % 2 == 0` is exactly `x.is_multiple_of(2)` — no overflow/sign edge cases for
`u64`. `u64::is_multiple_of` stabilized in Rust 1.87; the toolchain is
`dtolnay/rust-toolchain@stable` (1.93 locally) and the workspace pins no
`rust-version` MSRV, so this is safe. Behavior of the test RNG is unchanged.

### Removed msr.rs test imports — genuinely unused
`msr.rs:124` removed `use kvm_ioctls::{MsrExitReason, VcpuExit};`. I grepped the
whole file: zero remaining references to `MsrExitReason` or `VcpuExit`. Removal
is safe and is what clears the `-D warnings` gate.

### `flat_map(|(base, count)| (*base..*base + *count))` → unparenthesized
`msr.rs:141`. Dropping the redundant parens around the range expression is a
pure syntactic clippy fix; the produced range is identical.

### `ubuntu-24.04-arm` — valid hosted label for a public repo
GitHub's `ubuntu-24.04-arm` (and `ubuntu-22.04-arm`) hosted Arm64 runners are
available at no cost for public repositories. Since this repo is PUBLIC, the arm
leg of the matrix will schedule. KVM-requiring tests self-skip at runtime when
`/dev/kvm` is absent, so the arm leg exercises only the host-runnable suite as
intended.
