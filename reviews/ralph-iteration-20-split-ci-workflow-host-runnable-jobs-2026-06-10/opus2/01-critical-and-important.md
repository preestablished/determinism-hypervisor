# Critical & Important Issues

## CRITICAL — C1: aarch64 host leg will fail to compile (`dh-vmm` uses x86_64-only KVM bindings, ungated)

**Where:** `.github/workflows/ci.yaml:17` (the new `ubuntu-24.04-arm` matrix entry) in combination with `crates/dh-vmm/src/msr.rs:18-20`.

**What's wrong:**

The new matrix adds `ubuntu-24.04-arm` and the host job runs `cargo build --workspace` + `cargo test --workspace`. `dh-vmm` is a workspace member, so it is always compiled. But `dh-vmm` consumes x86_64-only `kvm-bindings` symbols **unconditionally**:

```rust
// crates/dh-vmm/src/msr.rs:18
use kvm_bindings::{
    kvm_msr_filter, kvm_msr_filter_range, KVM_MSR_FILTER_DEFAULT_DENY, KVM_MSR_FILTER_READ,
    KVM_MSR_FILTER_WRITE,
};
```

In `kvm-bindings 0.14.0`, `lib.rs` re-exports `self::x86_64::*` only under `#[cfg(target_arch = "x86_64")]` and `self::arm64::*` under `#[cfg(aarch64)]`. The MSR-filter types/constants are defined **only** in `src/x86_64/bindings.rs` — I confirmed `arm64/bindings.rs` contains **zero** occurrences of `kvm_msr_filter_range`, `KVM_MSR_FILTER_DEFAULT_DENY`, or `KVM_MSR_FILTER_READ`. On aarch64 these imports are unresolved and `cargo build --workspace` fails with `E0432 unresolved imports`.

`dh-vmm` has **no `cfg(target_arch)` gates at all** (grep found none in `crates/dh-vmm/src/`), and additionally hard-codes the x86 ioctl `KVM_X86_SET_MSR_FILTER` (`msr.rs:58`) and matches `VcpuExit::X86Rdmsr` / `VcpuExit::X86Wrmsr` (`kvm.rs:317,327`). This is fundamentally x86-only code.

**Why the local "all green" didn't catch it:** the verification ran on x86_64. The arm leg is brand-new in this change and was never executed. Every CI run on this branch (and on `main` after merge) will show the `host (ubuntu-24.04-arm)` job red.

**Fix — pick one:**

**Option A (recommended if arm coverage is genuinely wanted): gate `dh-vmm`'s x86 paths.** Make `dh-vmm` build to a thin stub on non-x86. At minimum:

```rust
// crates/dh-vmm/src/lib.rs — gate the KVM-bearing modules
#[cfg(target_arch = "x86_64")]
mod msr;
#[cfg(target_arch = "x86_64")]
mod kvm;
// ...and gate run.rs/vt.rs similarly, or move the portable math (agenda, vns,
// codecs) into modules that compile arch-independently.
```

This is non-trivial because the portable, arm-relevant logic (agenda math, rational math, detchannel) currently lives in the same crate as the KVM code. Splitting portable logic out is the clean long-term fix but is more than a CI change.

**Option B (smallest, unblocks CI now): drop the arm leg from this PR** and track the portability work as a follow-up bead. Revert `ci.yaml:17` to:

```yaml
    strategy:
      matrix:
        runner: [ubuntu-latest]
```

**Option C: keep the arm leg but exclude the non-portable crates on arm** via a per-runner package selection (e.g. `cargo build -p dh-proto -p dh-detclock -p dh-devices ...` on the arm matrix entry). This is brittle (the package list drifts) and I'd avoid it.

The PR comment at `ci.yaml:11-13` ("Runs on x86_64 and aarch64 — Spark-side devs touch shared code, so it must build/run on arm") states the intent, but the code does not yet support it. The intent is good; the prerequisite (arch-gating `dh-vmm`) is missing.

---

## IMPORTANT — I1: `kvm-intel` self-hosted lane has no `permissions:` block (token defaults to repo/org policy)

**Where:** `.github/workflows/ci.yaml:46-68` (`kvm-intel` job), and arguably the whole workflow.

**What's wrong:**

The fork-PR `if:` guard (`ci.yaml:49`) is correct and is the load-bearing control — good. But defense-in-depth for a self-hosted runner on a public repo also wants a least-privilege `GITHUB_TOKEN`. The workflow declares no `permissions:` block, so the token inherits the repository/organization default, which on many public repos is still read/write. A self-hosted job that runs *any* attacker-influenced code (even same-repo, if a maintainer's branch is compromised, or via a poisoned cached artifact on a persistent runner — see the research note on persistent-runner cache poisoning) should not also hold a write-capable token.

This is "important" not "critical" because the `if:` guard already blocks the primary fork-PR RCE path, and fork PRs get a read-only token regardless. But it's cheap insurance and the security research explicitly calls for minimal `permissions:`.

**Fix:** add a top-level read-only default:

```yaml
on:
  pull_request:
  push:

permissions:
  contents: read
```

(Both jobs only `checkout` and build; neither needs write.)

---

## Not an issue (checked and cleared)

- **`if:` expression correctness.** `github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository` evaluates correctly for: push to any branch (true, short-circuits — second term would be `null` on push but is never reached); same-repo PR (true); fork PR (false → skipped); re-run (GitHub preserves the original `event_name`/`event` context on re-run, so a re-run of a same-repo PR stays allowed and a re-run of a fork PR stays blocked). No gap.
- **`test -r /dev/kvm`** (`ci.yaml:64`) is the right probe: opening `/dev/kvm` for KVM ioctls requires read access for the runner user (group `kvm` typically grants `rw`). `-r` tests effective-UID readability, which is exactly the precondition the build's `Kvm::new()` needs. A stronger probe would be `-c /dev/kvm && -r /dev/kvm` (ensure it's a char device, not a stale regular file), but `-r` is sufficient and clear.
- **Clippy fixes are semantics-preserving.** See `03-positive-notes.md`.
