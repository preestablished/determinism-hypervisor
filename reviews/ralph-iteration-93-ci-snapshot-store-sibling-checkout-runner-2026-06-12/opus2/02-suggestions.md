# Suggestions (non-blocking)

### S-1 — Rebuild flow: Registration section does not point back to tool provisioning
**File:** `docs/ops/github-runner.md:93-108` (Registration section) / new section at 60-91

The Registration section is explicitly titled "(already done; for rebuilds)." On a real
rebuild, an operator follows it top-to-bottom: download runner, `config.sh`, install service —
and is *done*, with no signal that the new Tool provisioning section is also part of "rebuild
to working." The captured-PATH mechanism is especially easy to miss: a fresh `config.sh` on a
box where the tools were *not* yet installed would capture a `.path` without them, and the
M5/M6/M7 lanes would fail later for a non-obvious reason. Add a one-line pointer so the
rebuild story is complete.

```
The registration token (from `gh api`, requires repo admin) is single-use and
expires after 1 hour; `svc.sh` appears only after `config.sh` has run.
+
+On a from-scratch rebuild, install the tools in §"Tool provisioning" **before** running
+`config.sh` (or re-run it after), so the captured `.path` includes `~/go/bin` /
+`~/.cargo/bin`; otherwise the milestone lanes fail with a missing-tool error later.
```

---

### S-2 — Date-stamped Status column will silently rot
**File:** `docs/ops/github-runner.md:71` (table header "Status (2026-06-12)")

A status snapshot dated in the header is honest but will drift — nightly auto-updates
(line 85-87 says so), and a reader six weeks out cannot tell whether "✅ v1.9.3" is still
true. Consider a one-line "How to re-verify this table" pointer (the `go version -m`,
`cargo-fuzz --version`, `rustup toolchain list`, `stress-ng --version` commands are already
scattered in the prose) so a future operator can refresh the column without archaeology. Even
just: "_Re-verify with the per-tool commands in Notes; bump the header date when you do._"

---

### S-3 — `stress-ng` candidate version is recorded but not pinned at install
**File:** `docs/ops/github-runner.md:77`

The table helpfully records the apt candidate `0.17.06-1build1`, but the install command is
`sudo apt-get install -y stress-ng` (unpinned → whatever apt resolves at run time). For
parity with I-1 and because the exact version is already known, consider
`sudo apt-get install -y stress-ng=0.17.06-1build1` (and note it may need an `apt-mark hold`
if soak-load determinism matters — consistent with the kernel/microcode hold pattern the lock
file describes). Lower stakes than I-1 since stress-ng is a load generator, not a codec.

---

### S-4 — `grpcurl` dev-build verification command depends on a GNU-grep `\s` extension
**File:** `docs/ops/github-runner.md:84`

`go version -m ~/go/bin/grpcurl | grep '^\s*mod'` works on this box (I ran it — it returns the
`v1.9.3` mod line), but `\s` in grep is a GNU extension, not POSIX BRE; on a BSD/macOS grep it
would match a literal `s`. This runner is Linux-only so it is fine in practice, but for a copy-
pasteable runbook command `grep -E '^[[:space:]]*mod'` (or just `grep mod`, since the output
has one mod line for the main module) is more portable and equally correct.

---

### S-5 — "M6 smoke tests" / "M7 soak" lanes are provisioned ahead of any CI that invokes them
**File:** `docs/ops/github-runner.md:74, 77` ("Needed by" column)

Worth a half-sentence of honesty: I confirmed `ci.yaml`'s `kvm-intel` job currently runs only
`cargo build --workspace` / `cargo test --workspace`, `nightly-drift.yaml` runs the drift +
canary check, and **no** workflow yet invokes `grpcurl`, `cargo-fuzz`, or `stress-ng` (no
`crates/*/fuzz` targets exist either). The tools are pre-staged for lanes that do not exist
yet — reasonable for a runbook, but it means none of these installs is currently exercised by
a job, so "✅ installed" is not the same as "✅ works inside a runner job." A note like
"_provisioned ahead of the M5/M6/M7 lanes; not yet exercised by any workflow — first use will
validate the runner-job context_" sets the right expectation. (The `protoc` "Not needed" row,
by contrast, *is* exercised today, since every `cargo build` compiles `dh-proto`.)
