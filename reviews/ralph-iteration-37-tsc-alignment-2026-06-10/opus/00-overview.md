# Review Overview — ralph/iteration-37-tsc-alignment

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-37-tsc-alignment` vs `main`
- **Bead:** determinism-hypervisor-3np (P0, in progress)
- **Scope:** `crates/dh-vmm/src/tsc.rs` (new, 169 lines), `crates/dh-vmm/src/lib.rs` (+1 mod), `docs/decisions/tsc-alignment.md` (new, 45 lines)

## Verdict

**Request changes — one Critical soundness bug.**

The work is otherwise excellent: the benchmark answers the bead's question, the decision
is recorded before M4 freezes restore, the ioctl numbers and `_IOW` directions are correct
and well-justified, and the live round-trip test is convincing. But `get_tsc_offset` reads
the kernel's output back through a pointer derived from a **non-`mut`, shared (`&u64`)**
local with no `UnsafeCell`. That is undefined behavior in the Rust opsem model and a real
optimizer miscompilation hazard (the read-back can be constant-folded to the init value
under release/LTO). The fix is small and local. The test passing in a debug build is **not**
evidence of soundness — debug codegen does not exploit the `&`-shared / non-`mut` invariants.

## What I checked (sanity gate)

| Gate | Result |
|---|---|
| `cargo test -p dh-vmm tsc -- --nocapture` | **pass** — offset-attr **1117** ns/call, msr-write **1489** ns/call (N=10k) |
| `cargo test --workspace` | **pass** — all suites green (dh-vmm 68 + 7, others green) |
| `cargo clippy -p dh-vmm --all-targets` | **clean** — no warnings |
| `cargo fmt --check` | **clean** |
| ioctl 0xe1/0xe2/0xe3 vs `/usr/include/linux/kvm.h:1519-1521` | **match** — all three `_IOW` including GET (0xe2) |
| `vmm-sys-util` locked version | **0.15.0** (`ioctl_with_mut_ptr`/`ioctl_with_mut_ref` both available) |
| ARCH §4.4 / §8.3 "IA32_TSC ← vns" | confirmed (ARCH.md:367-368, 645, 683) |
| 1 GHz virtual-TSC convention | confirmed (ARCH.md:341-343) — see Suggestion on the unit note |

Benchmark numbers this run (1117 / 1489) differ from the doc (986 / 1591) by run-to-run
variance; the qualitative conclusion (offset-attr cheaper, set once, bit-exact) is unchanged
and the doc's numbers remain a fair representative sample. Not a blocker.

## Stats

| Category | Count |
|---|---|
| Critical | 1 |
| Important | 1 |
| Suggestions | 4 |
| Positive notes | 6 |

## File map

- `01-critical-and-important.md` — the `&u64` aliasing UB (Critical); `attr_for` provenance through int-cast (Important)
- `02-suggestions.md` — stray `let _ = &mut msrs;`, decision-doc unit note, benchmark-number drift, bead file-scope note
- `03-positive-notes.md` — what was done well
- `04-action-items.md` — checklist grouped Critical / Important / Suggestions
