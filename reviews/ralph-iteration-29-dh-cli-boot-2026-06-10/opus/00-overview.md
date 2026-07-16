# Review: `ralph/iteration-29-dh-cli-boot` — M0 boot path (bead `determinism-hypervisor-1mz`)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-29-dh-cli-boot` vs `main`
- **Commit:** `35548f1` (ralph: iteration 29 checkpoint — dh-cli boot subcommand, M0 acceptance LIVE)

## Verdict

**APPROVE WITH NITS.** The M0 boot path is correct, well-bounded, and the live
acceptance (hello → `HELLO\n`, pipeline_smoke → `K`) is sound. The code is
defensively written against hostile ELF input, the page-table / BootInfo / long-mode
setup matches ARCHITECTURE §2.3 and the nanokernel ABI, and the IN-FILL contract is
honored exactly. The bounds and overflow concerns raised in the brief all check out
clean.

One real correctness bug exists — the `--json` mode emits **invalid JSON** for any
non-printable serial byte (`\xNN` is not a legal JSON escape; verified empirically) —
but it is confined to a debug-only output path and does not affect the boot mechanism
or the acceptance tests. No blockers for merge; fix the JSON escaping before anyone
machine-parses `--json`.

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 1 |
| Suggestions | 7 |
| Positive notes | 8 |

| Area | Outcome |
|---|---|
| Long-mode entry (CR0/EFER/segments/no-IDT) | Correct for M0; robustness caveats documented (suggestions) |
| Page tables (2 MiB identity, 1 GiB cap) | Correct; arithmetic verified |
| ELF parsing (hostile input) | Robust; bounds + overflow handled via `get()` chains |
| IN-FILL contract adherence | Correct — INs answered on raw exit before classify |
| BootInfo ABI | Matches nanokernel canonical layout |
| `--json` escaping | **Invalid JSON for non-printable bytes** (Important) |
| `forbid(unsafe_code)` | Holds (crate-level in lib.rs + main.rs) |

## Files reviewed

- `tools/dh-cli/src/boot.rs` (new, 259 lines)
- `tools/dh-cli/src/lib.rs` (new)
- `tools/dh-cli/src/main.rs` (boot subcommand + arg parsing)
- `tools/dh-cli/tests/boot_hello.rs` (new, live kvm-gated)
- `tools/dh-cli/Cargo.toml`, `Cargo.lock`
- Context: `crates/dh-vmm/src/kvm.rs` (classify_exit IN-FILL contract, SlotVm),
  `tests/nanokernel/src/lib.rs` (BootInfo ABI), `ARCHITECTURE.md` §2.3 / §2.2 PIO map.
