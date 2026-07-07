# Step 2 — Frame-Cap Retune (Measurement-Derived) + Durable `linux_m5` Green

One lab session on the kvm-intel reference box. Stage artifacts per
`01-current-state.md` §Staging first; confirm the initramfs contains
`usr/bin/refwork-harness` and the image is at/after refwork `40eaf4f`.

## 1. Measure Instructions/Frame Yourself

Do not adopt the trail's ~25M or 27.8M figures — derive fresh numbers from the
currently staged image (the request is explicit about this).

Procedure (mirrors how take-two got its numbers):

1. Temporarily raise `FRAME_HARD_CAP` in `m5_frame_scheduling.rs` to a
   can't-fire value (e.g. `500_000_000` — the value take-two proved passes),
   and likewise `LINUX_FRAME_HARD_CAP` in `m5_net_loopback.rs`.
2. Run the Linux gates and capture the measurements:
   ```
   cargo test -p dh-worker --release --test m5_frame_scheduling -- --ignored linux_m5 --nocapture
   cargo test -p dh-worker --release --test m5_net_loopback   -- --ignored linux_ --nocapture
   ```
   Exact test names (verified): the `linux_m5` filter matches **two** tests
   in `m5_frame_scheduling.rs` — the gate
   `linux_m5_frame_budget_records_post_ready_frame_marks` (:55) and the
   NOP-game diagnostic
   `linux_m5_real_emulator_nop_game_frame_budget_diagnostic` (:240, also uses
   `FRAME_HARD_CAP` at :276) — both running is expected, not a surprise. The
   `linux_` filter in `m5_net_loopback.rs` matches
   `linux_pvblk_io_loopback_records_and_replays` (:54).
   Measurement sources differ per file: `m5_frame_scheduling` eprintln's the
   per-frame `(icount, frame)` table (`frame_marks()` at :747, printed at
   :226-231 and :341-343); `m5_net_loopback` prints only a single-line
   summary (`run_icount`/`frame_counter` at ~:171-175) — with
   `LINUX_IO_FRAMES = 1` the one-frame cost is `run_icount` directly.
3. Compute per-frame deltas for the fresh-boot run, the post-restore run, and
   the net-loopback IO frame. Record max and mean. Note: frame 1 from fresh
   boot includes post-READY warmup (~8.7M at take-two vs ~25M steady-state) —
   size caps on the **max** observed per-frame cost, not the mean.
4. Run the measurement **twice**; per the repo's determinism-lesson memory,
   icount-per-frame should be identical across runs (deterministic guest). If
   the two runs differ, stop and investigate before retuning — that's a
   determinism regression, not a tuning input.

## 2. Retune the Caps

For each cap, set `cap = ceil_to_round_number(max_per_frame × frames_covered × margin)`
with margin chosen so the final value is 2–4× the measured cost (the
acceptance criterion caps it at ≤4×; the follow-up's ≈150M for 3 frames at
~25M/frame ≈ 2× is the shape to match):

- `FRAME_HARD_CAP` (`m5_frame_scheduling.rs:47`): covers
  `max(FIRST_FRAMES=3, AFTER_RESTORE_FRAMES=2)` = 3 frames. At ~25–28M/frame,
  expect ≈150M–200M.
- `LINUX_FRAME_HARD_CAP` (`m5_net_loopback.rs:47`): covers
  `LINUX_IO_FRAMES = 1`. Expect ≈50M–100M — note the *current* 50M value may
  be numerically adequate for one frame; still re-derive it and rewrite the
  comment so the value is measurement-anchored rather than coincidental.
- `DETCHANNEL_FRAME_HARD_CAP` (`m5_frame_scheduling.rs:48`): per `01-`, the
  detchannel test is synthetic-guest only (verify this once more by reading
  its VM setup, `m5_frame_scheduling.rs:380-445`). If confirmed fixture-only:
  leave the value, but state that determination in the resolution file since
  the follow-up explicitly asked.

Each retuned constant gets a derivation comment of this shape, so the next
emulator-cost change is a one-line retune:

```rust
/// Measured 2026-07-07 against reference-workload dist image <hash/tag>
/// (refwork >= 40eaf4f): max ~NN M instr/frame (fresh-boot frames f1..f3,
/// post-restore f4..f5). Cap = NN M/frame x 3 frames x ~2 margin.
/// Retune by re-running: cargo test -p dh-worker --release \
///   --test m5_frame_scheduling -- --ignored linux_m5 --nocapture
const FRAME_HARD_CAP: u64 = 150_000_000;
```

Decision recorded here (the request offered the alternative): use **larger
constants**, not image-profile-aware (fixture vs real) caps. Rationale: the
caps are safety nets, not performance assertions; a single conservative value
keeps the test matrix simple, and the fixture path is unaffected by a larger
net. If the implementer finds a concrete reason to prefer profile-aware caps
(e.g. the fixture test *relies* on the cap firing), overrule this with a note
in the resolution.

## 3. Prove Non-Vacuousness and Fixture Neutrality

Acceptance criterion 1 has three legs:

1. **Real path stops on `BUDGET_REACHED`, not `HARD_CAP`:** the green runs in
   §4 must show stop reason `BUDGET_REACHED` / `frames_elapsed == budget`
   (the tests already assert this — `assert_worker_frame_budget`; just cite
   the passing run).
2. **The cap can still fire (≤4× rule):** state the arithmetic in the
   resolution: `cap / (max_per_frame × frames_covered) ≤ 4`.
3. **Fixture-profile tests pass unchanged:** run the non-ignored suites for
   the touched files without artifacts staged:
   ```
   cargo test -p dh-worker --release --test m5_frame_scheduling
   cargo test -p dh-worker --release --test m5_net_loopback
   ```

## 4. The Durable Green Gate + Evidence

With final cap values in the tree (not the temporary 500M):

1. Create `target/frame-cap-retune-<UTC timestamp, e.g. 20260707TnnnnnnZ>/`.
2. Run the Linux gates (`m5_frame_scheduling` linux_m5 + `m5_net_loopback`
   Linux test) with `--nocapture`, tee output into the evidence dir.
3. Per the determinism-lesson memory, run the frame-scheduling gate **3
   consecutive times**, all green, before calling it durable (cap changes are
   not hash-sensitive, but the gate's claim is "durably green", and reruns
   are cheap relative to a flaky exit-gate citation).
4. Write `00-evidence.md` in the dir: this repo's `git rev-parse HEAD`,
   dirty/clean state, the staged image identity (dist version + file hashes
   of bzImage/initramfs, e.g. `sha256sum`), refwork provenance (`40eaf4f` or
   later), the measured per-frame table, the cap arithmetic, and the raw log
   filenames.
5. This evidence dir doubles as the fresh Linux-replay-gate citation for the
   `02-` handoff — with staging already up, also run the corpus replay gate
   `linux_m5_record_replay_post_ready_corpus_reverifies`
   (`crates/dh-worker/tests/m5_record_replay.rs:123`):
   ```
   cargo test -p dh-worker --release --test m5_record_replay -- --ignored linux_m5 --nocapture
   ```
   and tee it into the same evidence dir.

Note: `target/` is not tracked by git — the durable record is the resolution
file citing the evidence dir path + key numbers (this matches the established
M9 evidence discipline).

## Acceptance for This Step

- Fresh measurement recorded (two identical runs); caps retuned with
  derivation comments; `DETCHANNEL_FRAME_HARD_CAP` determination documented.
- Real-emulator gates green ×3 with `BUDGET_REACHED`; fixture suites green.
- Evidence dir under `target/` with revs + image identity; numbers mirrored
  into the resolution files (`05-`).
- Bead created/claimed/closed for the retune; commit message cites
  `08-followup-frame-hard-cap.md`.
