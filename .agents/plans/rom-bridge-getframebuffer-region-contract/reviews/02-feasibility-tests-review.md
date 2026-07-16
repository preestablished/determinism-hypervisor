# Review 2: Implementation Feasibility / Test Coverage

Reviewer: subagent (feasibility + test-coverage lens), 2026-07-02, tree at `f58ac28`.

## Verification work performed

Read all 6 plan files and all 5 request files; read `tests/nanokernel/asm/capture_fixture.asm`,
`framebuffer_fixture.asm` (header), `tests/nanokernel/src/lib.rs`, `tests/nanokernel/build.rs`,
`tests/nanokernel/tests/elf_shape.rs`; audited every `capture_fixture_elf()` boot site and
`IcountBudget` in `crates/dh-worker/src/service.rs` and `crates/dh-worker/tests/m6_full_api_uds.rs`;
grepped the whole repo for `framebuffer_fixture`, `CAPTURE_FIXTURE_FB_BYTES`, `fb_info`/`fb_lz4`,
and golden-test pinning.

## Computed facts (load-bearing numbers)

- **Memory map**: channel at 0x40_0000, `CHANNEL_PAGES 512` (2 MiB) → channel spans
  0x40_0000–0x60_0000; FB at 0x60_0000. Resized FB ends at 0x60_0000 + 0x38000 = **0x63_8000**.
  Nothing else lives above 0x60_0000 in the fixture. Guest RAM is 8 MiB in both
  `capture_fixture_machine_config_with_epoch_len` (service.rs:5211, `8 * 1024 * 1024`) and m6
  (`const MEM: u64 = 8 << 20`, m6_full_api_uds.rs:38). The guest's own self-check
  (`cmp rax, FB_GPA + FB_BYTES`, capture_fixture.asm:84) auto-updates via the `%define`.
  **The resize is genuinely modest and safe** — plan claim verified.
- **Fill-loop icount**: the loop (capture_fixture.asm:124–128) retires 4 instructions/qword
  (`mov [rdi],rax / add rdi,8 / add rax,1 / loop`). Old: 8,192 × 4 = **32,768**. New:
  28,672 × 4 = **114,688** (+81,920), guest total-to-HLT ≈ 115–120k.

## Findings

**1. Critical — plan 04 ("Unit Tests" / plan 03 "Notes") — `capture_epoch_leg`'s
`IcountBudget(100_000)` becomes insufficient and the failure is runtime, not compile-time.**
`capture_epoch_leg` (service.rs:5274) boots capture_fixture and runs
`Until::IcountBudget(100_000)` (service.rs:5377). The fixture fills the FB **before**
publishing the manifest and CHANNEL_INIT; after the resize the fill alone needs ~114,688
instructions, so the run stops mid-fill with no manifest attached. The `capture=true` leg
then calls `capture_at_boundary(...).unwrap()` (service.rs:5404) → panic, and
`assert_eq!(out.feature_bytes, capture_fixture_bytes(8, 24))` can never pass. This breaks
`m6_accept_capture_neutrality_and_layout_precondition` (service.rs:7482) — the C5 acceptance
test. The plan says only "re-check any capture.framebuffer assertions" around line 5403 and
elsewhere claims "compile errors from the removed helpers will point at every site" — this
site has **no** compile error (`capture_fixture_spec` survives). The plan must explicitly
say: raise the budget (≥ ~130k; 500k–1M is safe; note epoch_len=64 so epoch count grows
accordingly, which the neutrality assertions tolerate since both legs match).

**2. Important — plan 01/04 fixture-deletion checklist misses
`tests/nanokernel/tests/elf_shape.rs`.**
Plan 01 asserts "No other test file references GetFramebuffer or the framebuffer fixtures
(verified by grep across `crates/`)" — the grep scope missed `tests/nanokernel/tests/`.
Deleting framebuffer_fixture requires touching: `assert_guest_shape("framebuffer_fixture", ...)`
(elf_shape.rs:61), `framebuffer_fixture_asm_matches_rust_constants` (elf_shape.rs:425–498),
and the `"framebuffer_fixture.asm"` entry in `channel_guest_asm_ring_descs_match_the_constant`
(elf_shape.rs:511). Compile errors will surface most of it (the deleted
`framebuffer_fixture_elf` import), so an agent recovers — but the plan's "complete inventory"
claim is false, and the string-literal entry at line 511 fails only at test time.

**3. Important — plan 04 leaves the m6 `capture_spec()` question open when the answer was
one grep away.**
`capture_spec()` sets `framebuffer: false` (m6_full_api_uds.rs:135), so m6's assertion that
`fb_lz4`/`fb_info` are empty (line 507) and the leg-digest hash of `fb_lz4` (line 563,
cross-leg comparison, not a pinned golden) are **unaffected**; `expected_capture_bytes()`
auto-adapts via the constant. The plan should state this so the implementer doesn't burn a
KVM+64-core-gated run chasing it. Same for the unresolved "check whether capture_spec() sets
framebuffer: true" in plan 01.

**4. Important (verified in the plan's favor) — golden/hash tests do not pin capture bytes.**
The claim "nothing in the repo's golden/entr tests is known to pin the old zero-FbInfo bytes"
checks out: `entr_golden.rs` uses entropy_draw; `crates/dh-snapshot/tests/golden.rs`,
`crates/dh-inputlog/tests/golden.rs`, `snapshot_engine.rs`, and determinism-sensitive
integration tests have zero `capture_fixture`/`fb_info`/`fb_lz4` references. The only files
outside `service.rs` touching these are `m6_full_api_uds.rs` and
`tests/nanokernel/tests/capture_manifest_interop.rs` (the latter derives everything from
`CAPTURE_FIXTURE_FB_BYTES` and auto-adapts).

**5. Minor — plan 01's runtime-test inventory omits four capture_fixture tests.**
`m6_accept_capture_neutrality_and_layout_precondition` (7482 — the one that breaks, see
finding 1), `run_capture_layout_mismatch_commits_successful_run_boundary` (~7664),
`take_snapshot_capture_checks_layout_version_and_returns_features` (~7695),
`verify_replay_rpc_handles_detchannel_capture_fixture_log` (~7792). I verified the latter
three use `framebuffer: false` and 10M budgets, so they survive the resize unchanged — but
an inventory claiming completeness should list them, and the layout-mismatch tests'
expectations (`err.message().contains("layout_version")`) remain valid under the new
contract.

**6. Minor — elf_shape's `%define` parser trap for the new size literal.**
`capture_fixture_asm_matches_rust_constants` parses defines with
`u64::from_str_radix(hex,16)` or `v.parse()` (elf_shape.rs:364–376). NASM accepts `229_376`
with an underscore; Rust's `parse()` does not — the drift test would panic. Write the asm
define as `229376` or `0x38000`. (Fails loudly, so not blocking — but worth one sentence in
plan 04.)

**7. Nit — stale doc comment.**
`docs/ops/m6-grpcurl-metrics-smoke.md:146` says "The landing-loop fixture has no framebuffer
descriptor" — the behavior (FailedPrecondition, no region) is unchanged, but "descriptor"
wording goes stale. Plan 03 step 5 greps for `descriptor` only within service.rs; plan 05
checks `docs/upstream-divergences.md` (which exists and has no framebuffer entry → "no
change" branch applies) but not `docs/ops/`.

**8. Process completeness (plans 03/05) — sound.**
`build.rs` has `cargo:rerun-if-changed` on the whole `asm/` dir and the `PROGRAMS` list
(build.rs:20 needs the `"framebuffer_fixture"` entry removed — plan 04 does say "the
build-script entry that assembles it"), so rebuild mechanics are automatic. The bd commands
match repo conventions; `bd ready` currently shows no open issues, so no collision.
`docs/decisions/` exists with the named siblings. Clippy, the 3× workspace-run determinism
gate, the m9_handoff.rs dirty-tree warning, and the "never report an ungated pass" rule are
all correctly captured. The `m6_full_api_uds` 64-core gate caveat is honest.

## Verdict

**Yes, a competent agent can implement from this plan without getting blocked** — the
code-change sequence (files 02/03) is accurate against the tree, the fixture resize is
confirmed feasible (memory map clean, 8 MiB RAM covers 0x63_8000, guest self-check
auto-updates), and the deletion inventory is ~90% complete with compile errors catching most
of the rest. But one landmine will cost real time: the capture-neutrality test will fail at
KVM-runtime with a confusing "detchannel/manifest" capture panic, not a compile error, and
the plan's guidance actively points away from it.

**Top 3 fixes:**
1. Add to plan 04: raise `capture_epoch_leg`'s `IcountBudget(100_000)` (service.rs:5377) to
   ≥ ~500k — the resized fill loop alone retires ~114,688 instructions before the manifest
   is published, so 100k stops mid-fill and the capture leg panics (finding 1).
2. Add `tests/nanokernel/tests/elf_shape.rs` (lines 61, 425–498, 511) to the
   framebuffer_fixture deletion checklist, and correct plan 01's "verified by grep across
   crates/" claim (finding 2).
3. Resolve the m6 open question in the plan: `capture_spec()` sets `framebuffer: false`
   (m6_full_api_uds.rs:135), so m6 needs no framebuffer-assertion changes — only the
   auto-adapting constants (finding 3). Optionally note the `229_376`-underscore parser trap
   in the asm define (finding 6).
