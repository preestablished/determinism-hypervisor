# Current State — Verified Against the Tree (2026-07-07, main @ `4497f60`)

Everything below was re-verified in this repo (and guest-sdk's local checkout)
while writing this plan; line numbers are from `4497f60` (current HEAD; the
request text says `bdd476b`, which is a few commits behind — `runctl.rs`
cites shift by ~4 lines between the two).

## The Cap Constants (item 1 target surface)

| Constant | Value | Location | Governs |
|---|---|---|---|
| `FRAME_HARD_CAP` | `50_000_000` | `crates/dh-worker/tests/m5_frame_scheduling.rs:47` | `hard_icount_cap` for the Linux M5 frame-budget runs (`FIRST_FRAMES = 3` fresh, `AFTER_RESTORE_FRAMES = 2` post-restore); also used by the ignored NOP-game diagnostic (:276) and the **non-ignored fixture helper** `run_frames` (:731, callers :973/:1056) where it is a can't-fire cap under `Until::FrameBudget` asserting `BudgetReached` — raising it is safe for the fixture path (no test asserts a `HardCap` stop with these constants) |
| `DETCHANNEL_FRAME_HARD_CAP` | `1_000_000` | `crates/dh-worker/tests/m5_frame_scheduling.rs:48` | detchannel frame test, used at lines 436 and 479 |
| `LINUX_FRAME_HARD_CAP` | `50_000_000` | `crates/dh-worker/tests/m5_net_loopback.rs:47` | `hard_icount_cap` for the Linux pv-blk IO run, `LINUX_IO_FRAMES = 1` (line 46), used at line 77 |

**`DETCHANNEL_FRAME_HARD_CAP` does not gate the real-emulator path.** The
detchannel test (`detchannel_frame_budget_drains_sdk_frame_marks_across_restore`)
builds its own synthetic VM via `CreateVm` with a nanokernel fake-frame guest
(see `m5_frame_scheduling.rs:380-445`); it never loads the M9 Linux image. Per
the request ("if it gates the real-emulator path"), the expected action is
*verify and leave unchanged*, documented in the resolution — not retune.

## Measurement Trail (instructions/frame, real emulator)

- Take-two `08-followup-frame-hard-cap.md` (2026-07-06): observed frame table
  `[(8724677,1),(33929802,2),(57954297,3)]` fresh and
  `[(24024950,4),(48049735,5)]` post-restore → ~24–25M instr/frame; test
  passed fully with cap temporarily at 500M; recommends cap ≈150M for 3
  frames.
- Bead `determinism-hypervisor-38b6` note (2026-07-07): newer figure **27.8M**
  instr/frame.
- The request says: trust your own fresh measurement over both.

## Run-Loop Facts (item 4 — the backstop question)

- **No in-kernel irqchip, by design.** `crates/dh-vmm/src/lib.rs:170-189`
  lists `no_in_kernel_irqchip` in the forbidden-capability assertions;
  `crates/dh-vmm/src/kvm.rs:960` documents "We never call
  KVM_CREATE_IRQCHIP/KVM_CREATE_PIT2" with a smoke test. **Consequence:** KVM
  cannot emulate HLT in-kernel — every guest `HLT` exits to userspace as
  `KVM_EXIT_HLT`, so `KVM_RUN` returns. There is no kernel-side "block until
  interrupt" path for this VMM.
- **Every HLT stops the run.** `crates/dh-vmm/src/runctl.rs:632`: the exit
  handler matches `VcpuExit::Hlt` → sets `halted` → unwinds to
  `StopReason::GuestHalted` (~line 734). The code does **not** distinguish
  idle HLT (IF=1, waiting for a tick) from terminal HLT — both return
  `GuestHalted`. So the request's suspect case (a), "idle HLT blocks inside
  KVM_RUN", is analytically impossible here; the run *returns*, it does not
  hang. (Whether `GuestHalted` is the *right answer* for an idle-parked guest
  is a semantics question for the resolution note, not a hang.)
- **Suspect case (b), non-HLT zero-retirement block:** with no in-kernel
  irqchip/PIT and no kvmclock, the known in-kernel blocking sources are
  absent. Candidate guest states (MWAIT, PAUSE-loop) either exit or retire
  instructions (icount cap trips). This is the case the step-0 repro must
  actually probe; the prior is "no hang" but it has not been demonstrated.
- `Until::NextSdkEvent { hard_cap }` at `runctl.rs:79`; hard cap plumbed via
  `FinalStop::HardCap` at `runctl.rs:538`.

## DHILOG / Replay Surfaces (item 3 — handoff raw material)

Contract elements from the two guest-sdk beads, with where each lives today:

| Contract element | Code | Notes |
|---|---|---|
| `PAD_SET` records | `crates/dh-inputlog/src/dhilog.rs:44` (`KIND_PAD_SET = 0x01`), writer at :171 | |
| `DEV_EVENT` records | `dhilog.rs:45` (`KIND_DEV_EVENT = 0x02`), writer at :193 | |
| Ring C/I pushes | `dhilog.rs:67` (`EVENT_RING_PUSH = 0x0001`); emitted at `crates/dh-devices/src/detchannel.rs:797` | |
| Ring A/W consumer bumps | `dhilog.rs:68` (`EVENT_CONS_BUMP = 0x0002`); emitted at `detchannel.rs:800-806` (`fn cons_bump`) | The request called this "the known question mark" — it **does** exist; the verification task is coverage/encoding fidelity vs. the guest-sdk contract, not existence |
| `pio_answer` | `dhilog.rs` (`EVENT_PIO_ANSWER = 0x0003`, `pio_answer()` writer at :218-232); encoding test `pio_answer_dev_event_encoding` at :537 | |
| Replay-mode application | `crates/dh-worker/src/replay_engine.rs` — applies `PadSet`/`DevEvent`; `decode_pio_answer` (:338), divergence reporting `pio_answer_missing`/`pio_answer_mismatch` (:174, :185), `EVENT_CONS_BUMP` verification (:611, :913) | |
| Bit-identical Linux replay gate | M9 evidence `target/m9-final-acceptance-20260621T004402Z/17-linux-m5-corpus.log`; Linux M5 corpus work in `.agents/plans/resolve-4s9-27-linux-m5-corpus/` | Needs a *fresh* rerun citation, not just the June evidence, if guest-sdk wants current-image coverage |

guest-sdk is a **local sibling checkout** at `~/git/preestablished/guest-sdk`
with its own beads DB (`bd show guest-sdk-ext-hyp-input-log-dev-events` works
from that directory) and an established `.agents/requests/` +
`.agents/reviews/` structure for cross-repo filings. Both beads: P0, BLOCKED,
last updated 2026-06-18, unblock condition "shipped **and available to the
Intel VM lane**".

## Artifact Staging for the Lab Lane (items 1–3 all need this)

From bd memories (verified conventions, 2026-07-07):

- `DH_M9_BZIMAGE` + `DH_M9_INITRAMFS` → from
  `reference-workload/dist/workload-image-0.1.0/` (decompress
  `initramfs.cpio.zst`; the initramfs **must contain executable
  `usr/bin/refwork-harness`** and a `boot.toml` autostart exec'ing it).
  The `~/.cache/dh-m9/reference-workload/initramfs.cpio` is the OLD contract
  fixture (`opt/m9-refwork-contract` autostart) and is **rejected** by
  `assert_m9_real_emulator_initramfs`
  (`crates/dh-worker/tests/common/mod.rs:248`, called from
  `m9_linux_ready_snapshot` at :649).
- `DH_M9_BASE_IMAGE` / `DH_M9_GAME_IMAGE` → from
  `~/.cache/dh-m9/reference-workload/`.
- `DH_M9_IMAGE_CACHE=~/.cache/dh-m9/image-cache`.
- Test invocation shape:
  `cargo test -p dh-worker --release --test m5_frame_scheduling -- --ignored linux_m5`
  (hardware-gated: kvm-intel + dirty-ring). **Skip semantics:** missing
  artifacts/dirty-ring make the harness **error**, not skip, unless
  `DH_M9_ALLOW_SKIP=1` is set (`common/mod.rs:61,:169`; the error text notes
  ALLOW_SKIP is not accepted for final M9 gates — fine for iteration, do the
  evidence runs with a complete environment). `DH_M9_GUEST` exists but is not
  consumed by these m5 tests.
- **Image freshness check before measuring:** the caps question came from
  reference-workload fix `40eaf4f` (refwork-harness on `SdkPlatform`). Confirm
  the staged dist image is at/after `40eaf4f`; coordinate with `refwork-gp9`
  (image rebuild) only in the sense of using the freshest available image —
  do not block on it (request: "a green run against the newest
  `40eaf4f`-fixed image is the substance").

## Prior Trail (for citations in resolutions)

- `requests/phase3-snapshot-restore-no-frame-under-no-tick-take-two/07-handoff-resolution.md`
  — root cause localized to refwork (`refwork-4qj`, fixed `40eaf4f`);
  `08-followup-frame-hard-cap.md` — the surviving cap-retune ask.
- `requests/nextsdkevent-run-wallclock-backstop/00-request.md` — the
  confirm-first backstop filing; no resolution file yet.
- Worker-side drain fix `4b19c52`; test observed passing at `ecd60ae` with a
  temporarily-raised cap.
- Evidence-dir discipline: timestamped dirs under `target/` (e.g.
  `target/m9-final-acceptance-20260621T004402Z/`).
