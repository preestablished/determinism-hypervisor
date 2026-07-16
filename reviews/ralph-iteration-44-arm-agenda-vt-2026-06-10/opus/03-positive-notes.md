# Positive Notes

- **Behavior-preserving git-mv.** Moving the CLI into a lib module via `git mv` (so the diff renders as a rename) with only `dh_cli::`→`crate::` and `fn main`→`pub fn main` changes is exactly the right way to make this reviewable. Confirmed zero leftover `dh_cli::` refs and all 5 `crate::` rewrites correct. The thin `main.rs` arch-dispatch (`dh_cli::cli::main()` on x86, honest stderr+exit 2 elsewhere) is clean.

- **Honest non-x86 failure instead of compile-out.** dh-worker's `kvm_checks()` returns a real failing `CheckResult` on non-x86 (`got: target_arch=aarch64`, `want: x86_64 (VMX)`) rather than silently disappearing. This keeps `run_preflight()` total and truthful on every arch — the right call for a preflight gate. Using `std::env::consts::ARCH` for `got` is sane and the `&str`→`String` `.into()` on `want` is correct against the `got: String` field.

- **Dependency gating is correct and minimal.** Moving kvm-bindings/kvm-ioctls/libc/vm-memory/vmm-sys-util into `[target.'cfg(target_arch = "x86_64")'.dependencies]` (and nanokernel into the gated dev-deps) is the standard idiom. Cargo.lock is unchanged because Cargo locks all platforms regardless of cfg — so the x86 kvm-intel lane resolves bit-identically. No feature-unification trap (dh-devices never pulls vm-memory).

- **Determinism math genuinely freed.** agenda/vt/config/blkfile stay unconditional with ungated `#[cfg(test)]` modules; the arm lane now exercises 26 of those unit tests plus all shared-crate suites. The gated KVM modules and test targets correctly compile to empty there. This is the real, intended payoff of the bead.

- **Excellent inline rationale.** Every gate carries a comment explaining *why* (bead v5w, which x86-only API forces the gate, what the arm lane covers). The ci.yaml comment was rewritten to match reality, and the load-bearing nasm-install and fmt-member-scoping steps were left intact. This is the kind of self-documenting change that future maintainers can trust.

- **Tests prove coverage is preserved, not just asserted.** 195/195 on x86, and the diff provably adds/removes zero `#[test]` functions — so the inner `#![cfg]` additions are no-ops on x86 and pure enabling on arm.
