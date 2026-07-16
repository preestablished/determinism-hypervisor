# Positive notes

- **The fork guard is the right call, and it's the *hard* one to get right.**
  Refusing to run untrusted fork code on a runner with `/dev/kvm` and full lab
  access is exactly correct. The `head.repo.full_name == github.repository`
  check is the standard, robust way to express "same-repo only." My only ask is
  to *document* the consequence (I1); the security decision itself is sound.

- **Comparator fails closed in every direction I tested.** Missing lock → exit 1;
  zero parseable keys → exit 1 ("comparator or lock is broken"); any single-key
  drift → exit 1 with a precise `lock=... live=...` per-key report and a pointer
  to the re-baseline procedure. No path silently passes a broken comparator.
  Verified live (7 keys green on this box) and doctored (drift caught).

- **shellcheck-clean** with `set -euo pipefail`, proper quoting throughout,
  first-`=`-split parsing (`${line%%=*}` / `${line#*=}`) that correctly handles
  values containing `=` and the `cpu_brand` value with spaces/parens.

- **`$(dirname "$0")` lock resolution** means the script finds its lock
  regardless of the caller's cwd — so `bash repo/ci/check-determinism-class.sh`
  from the workflow's default cwd resolves `repo/ci/determinism-class.lock`
  correctly. Nice, and it's why the nightly's drift step doesn't need a
  `working-directory`.

- **The canary is a genuinely good second layer.** The header comment nails the
  rationale: the host-tuple lock cannot catch a KVM *behavior* change inside the
  same kernel package, so running the 1e9-twice regression + counting-semantics
  empirics nightly catches semantic drift the lock structurally can't. `needs:
  determinism-class` ordering is sensible (don't burn the long suite if the host
  already drifted).

- **Both new workflows parse** (validated via `yaml.safe_load`), the three
  canary test targets exist and compile (`determinism-tests` package:
  `regression`, `counting_semantics`, `counting_smoke`), and the `/dev/kvm` rw
  precheck + `nasm` precheck mirror ci.yaml's loud-fail-on-provisioning-drift
  pattern.

- **CONTRIBUTING explains the *why*, not just the rules.** The "a red
  determinism job is NEVER worked around — a divergence is a P0, a
  counting-semantics failure triggers the BR_INST_RETIRED fallback decision, not
  a patch-around" framing is exactly the culture this product needs documented.

- **Comments are honest about the sharp edges** — e.g. the cargo-fmt
  `--package` scoping note (why not `--all`), and the lock's own "vmm_version is
  deliberately absent / code-side" note. This is the kind of context that
  prevents a future agent from "fixing" something that's correct.
