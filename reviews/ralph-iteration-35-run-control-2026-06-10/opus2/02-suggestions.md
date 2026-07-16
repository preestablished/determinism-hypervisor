# Suggestions

## S1 — Deferral crossing the next agenda point → OVERSHOOT (deterministic-loud, but wrong semantics)

**Where:** `runctl.rs:181-264` (the outer agenda loop) + `inject.rs:147` (`max_defer_steps = epoch_len`).

If an injection at agenda point *k* defers forward (single-stepping while the window is closed) **past** the icount of point *k+1*, the outer loop then reaches point *k+1* and calls `land_at(point_{k+1}.icount)` with `point_{k+1}.icount < counter.read()` → `BoundaryError::Overshoot` → fatal `RunError::Boundary`. The deferral budget is `epoch_len` (50M default), so a deferral can run far past the next epoch/poll/injection point.

This is deterministic and loud (good — never silent corruption), but the *semantic* is wrong: §3.4 says "deliver at the first injectable boundary ≥ B," which legitimately can be past the next agenda point; the engine should skip/re-plan the points the deferral stepped over, not crash. The agenda↔deferral interplay is unspecified in ARCH §3.3/§3.4. **Recommend:** file an M6-scheduler bead to define this (e.g. after an injection, drop or re-base agenda points whose icount ≤ delivered_icount). Latent now (no Phase-1 injection path), same as the Critical.

## S2 — Assert `counter.read() == start_icount` at segment entry (caller-lie guard)

**Where:** `runctl.rs:143-178`. `Segment.start_icount` is caller-asserted and feeds the agenda grid (`agenda.rs` epoch/poll/final all relative to it), while `land_at` uses absolute `counter.read()` values. If the caller passes `start_icount != counter.read()`, every agenda point lands at the wrong place (or immediately overshoots). Cheap, high-value guard: at `run_segment` entry, `let c = seg.counter.read()...; if c != seg.start_icount { return Err(...) }`. Turns a silent mis-landing into a loud precondition violation. `dh-cli run` happens to pass `start_icount: 0` after `counter.reset()`, so it's correct today, but the invariant is undocumented and unchecked.

## S3 — Pause-flag racy observation point: confirm the doc covers it, and consider `Acquire` for the hash read

**Where:** `runctl.rs:239` (`seg.pause.load(Ordering::Relaxed)`).

The analysis holds: `Relaxed` gives eventual visibility, the rolled-to epoch boundary is deterministic *given* the observation point, and ARCH §3.3 + API.md §2.4 explicitly bless "pause soon at a deterministic point" with `Pause` as an external async input not replayed by verification. So the racy *which-point-do-we-observe-pause-at* is acceptable **by design** and the docs DO state it (ARCH §3.3: "externally-caused pauses on the deterministic grid"; API.md §2.4 PauseResponse comment). No correctness bug.

Minor: the value the pause path then hashes (`push_final_link`, runctl.rs:253-255) is read after the `Relaxed` load with no fence. For a single producer flag this is fine, but if the pausing thread also publishes data the hash depends on, `Relaxed` wouldn't order it. Phase-1 has no such coupled data, so leave as-is; just be aware if M6 adds a pause-payload.

## S4 — `Until::Goal` callback determinism is the caller's burden — document it

**Where:** `runctl.rs:141-147` (the `goal: &mut dyn FnMut() -> bool` param). For replay identity the goal's return must be a pure function of guest state at the poll boundary. M6's gRPC goals read guest memory regions (deterministic), but the Phase-1 closure is arbitrary host code — a goal that consulted wall-clock or a host counter would fork replays silently. The doc comment doesn't say the closure must be deterministic. Add one sentence to the `goal` param doc. (`dh-cli run` passes `|| false`, trivially fine.)

## S5 — `finish()` double-hash when epoch and final coincide

**Where:** `runctl.rs:218-235` then `finish()` at `runctl.rs:268-287`. When the final stop point also lands on an epoch multiple, `point.epoch_hash` is true so `push_final_link` runs at runctl.rs:221, and then `finish()` calls `push_final_link` **again** at the same boundary (runctl.rs:277). ARCH §8.5 says the chain is "computed at every epoch boundary **and** at every final pause" — so two links at a coincident boundary may be intentional (epoch link, then final-pause link). But it's worth a one-line comment confirming the double-link is by design, because it doubles the chain length at every epoch-aligned stop and a future reader will suspect a bug. If it is NOT intended, guard `finish()` against re-hashing a boundary already hashed this iteration. Verify against the M6 verification-mode expectation.

## S6 — `gettid()` main-thread assumption in `dh-cli` is correct but fragile

**Where:** `tools/dh-cli/src/run.rs:613-617`. `std::process::id() as i32` works *only* because the CLI runs everything on the main thread, where tid==pid. The comment says so. Fine for the CLI. Note the dh-vmm test rigs use the real `SYS_gettid` syscall (e.g. `runctl.rs:297-303`); the CLI's no-unsafe constraint forces the pid shortcut. If `dh-cli run` ever spawns the run on a worker thread, the PMI overflow signal would route to the wrong thread and landings would hang/misbehave. Add an assertion or comment guard if threading is ever introduced.
