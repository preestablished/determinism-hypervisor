# Positive Notes

### Correct, textbook self-hosted fork-PR gate
`ci.yaml:49`
```yaml
if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository
```
This is precisely the guard recommended for a public repo with a self-hosted
runner. It admits same-repo pushes and same-repo PRs while excluding fork PRs,
and the `||` short-circuit avoids evaluating `github.event.pull_request` on push
events. The inline comment (`ci.yaml:47-48`) states the threat model plainly.

### Defense-in-depth `/dev/kvm` check
`ci.yaml:64`
```yaml
- run: test -r /dev/kvm || { echo "::error::/dev/kvm not available on runner"; exit 1; }
```
Turns a silent false-green (live-KVM tests self-skipping) into a loud failure
with a GitHub error annotation. This is the right instinct for a hypervisor
project where the whole point of the `kvm-intel` lane is to actually exercise
KVM.

### Lint fixes preserve semantics exactly
- `agenda.rs:408-410` — `% 4 != 0` → `!is_multiple_of(4)` and `% 2 == 0` →
  `is_multiple_of(2)` on a `u64` RNG output; mathematically identical, more
  expressive.
- `msr.rs:124` — removed imports verified to have zero remaining references.
- `msr.rs:141` — redundant range parens dropped; range unchanged.

The branch raised the bar by adding `cargo clippy --workspace --all-targets -D
warnings` as a gate (`ci.yaml:36`) and then made the tree pass it, rather than
just turning the gate on and leaving warnings.

### Path-dependency checkout matches the documented sibling pattern
`ci.yaml:23-30, 55-62`. The `guest-sdk` checkout mirrors the existing
`control-plane` pattern and is documented in both the workflow header
(`ci.yaml:6-8`) and the root `Cargo.toml` comments (lines 23-26), satisfying the
"CI owns the sibling checkout story" guidance from the path-deps research. This
is the actual fix that unblocks the red `main` after iteration 19.

### Clear, intent-revealing comments
The workflow header and per-job comments explain *why* there are two lanes
(host-runnable vs KVM-gated), what each exercises, and why arm is in the matrix
(Spark-side devs touch shared code). Good signposting for the next maintainer.
