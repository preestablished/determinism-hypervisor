# Review Resolution (2026-07-07)

Two independent subagent reviews (see `reviews/01-` and `reviews/02-`), both
verdicts: no blockers. All findings were accepted and applied to the plan
files, except where noted. Disposition:

| Finding | Applied? | Where |
|---|---|---|
| R1.1 / rev baseline `bdd476b` vs `4497f60` | Yes | 00 intro, 01 header |
| R1.2 = R2.9 / `common/mod.rs:609` mis-cite | Yes | 01 §Staging (now `assert_m9_real_emulator_initramfs` :248, called :649) |
| R1.3 / net_loopback prints summary, not frame table | Yes | 03 §1.2 (per-file measurement sources) |
| R1.4 / hard-cap-under-NextSdkEvent test already exists (`runctl.rs:2340-2362`) | Yes | 04 Probe B — cite, don't rebuild |
| R1.5 / `FRAME_HARD_CAP` extra call sites (:276, `run_frames` :731) | Yes | 01 cap table |
| R1.6 / cite capture-watchdog precedent (`service.rs:1493-1568`) | Yes | 04 decision-gate resolution guidance |
| R2.1 / pin exact Linux test names + two-tests-under-`linux_m5` | Yes | 03 §1.2 |
| R2.2 / name the corpus replay gate (`m5_record_replay.rs:123`) | Yes | 02 matrix row, 03 §4.5 (with invocation) |
| R2.3 / cite guest-sdk contract docs; element-level baseline | Yes | 02 §1.2, §2 method |
| R2.4 / handback dir → `phase3-ext-hyp-input-log-and-replay-handoff/` | Yes | 02 §4.1 |
| R2.5 / `--append-notes`, not `--notes` | Yes | 02 §4.2 |
| R2.6 / `DH_M9_ALLOW_SKIP=1` semantics; `DH_M9_GUEST` unused | Yes | 01 §Staging |
| R2.7 / HANDOFF bead closes on guest-sdk ack, not on filing | Yes | 02 acceptance, 05 §Beads |
| R2.8 / `-l` labels on `bd create` | Yes | 05 §Beads |
| R2.10 / spell `.agents/requests/` paths | Yes | 05 §Resolutions |

Nothing was rejected. Both reviewers independently confirmed the plan's
load-bearing claims: the cap constants and their call sites, the
synthetic-only nature of the detchannel test, the DHILOG/replay surface
inventory (including ring A/W cons-bump existence), the
no-in-kernel-irqchip → HLT-exits-to-userspace → `GuestHalted` chain, the
resolution-file numbering in all three request dirs, and the existence/state
of both guest-sdk beads.
